use futures::StreamExt;
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{Container, PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use k8s_openapi::chrono::Utc;
use kube::{
    runtime::controller::{Action, Controller},
    Api, Client, CustomResource, CustomResourceExt, Resource, ResourceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::time::Duration;

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct BuildCondition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition_time: Option<String>,
    pub observed_generation: Option<i64>,
}

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "build.example.com",
    version = "v1alpha1",
    kind = "NixBuild",
    namespaced,
    status = "NixBuildStatus",
    printcolumn = r#"{"name":"status", "jsonPath":".status.phase", "type":"string"}"#,
    printcolumn = r#"{"name":"age", "jsonPath":".metadata.creationTimestamp", "type":"date"}"#
)]
pub struct NixBuildSpec {
    pub git_repo: String,
    pub git_ref: Option<String>,
    pub nix_attr: Option<String>,
    pub image_name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct NixBuildStatus {
    pub phase: String,
    pub job_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub conditions: Vec<BuildCondition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<String>,
}

impl NixBuildStatus {
    fn needs_update(&self, new_status: &NixBuildStatus) -> bool {
        self.phase != new_status.phase
            || self.job_name != new_status.job_name
            || self.message != new_status.message
            || self.observed_generation != new_status.observed_generation
            || self.conditions.len() != new_status.conditions.len()
            || self
                .conditions
                .iter()
                .zip(new_status.conditions.iter())
                .any(|(a, b)| a.status != b.status || a.message != b.message)
    }

    fn set_condition(&mut self, type_: &str, status: &str, reason: &str, message: &str) {
        let now = Utc::now().to_rfc3339();

        if let Some(existing) = self.conditions.iter_mut().find(|c| c.type_ == type_) {
            if existing.status != status || existing.message != message {
                existing.last_transition_time = Some(now.clone());
                existing.status = status.to_string();
                existing.reason = reason.to_string();
                existing.message = message.to_string();
                self.last_transition_time = Some(now);
            }
        } else {
            self.conditions.push(BuildCondition {
                type_: type_.to_string(),
                status: status.to_string(),
                reason: reason.to_string(),
                message: message.to_string(),
                last_transition_time: Some(now.clone()),
                observed_generation: None,
            });
            self.last_transition_time = Some(now);
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Kube Error: {0}")]
    KubeError(#[from] kube::Error),
    #[error("Build Error: {0}")]
    BuildError(String),
}

struct ContextData {
    client: Client,
}

async fn reconcile(build: Arc<NixBuild>, ctx: Arc<ContextData>) -> Result<Action, Error> {
    let ns = build.namespace().unwrap_or_else(|| "default".into());
    let builds: Api<NixBuild> = Api::namespaced(ctx.client.clone(), &ns);
    let jobs: Api<Job> = Api::namespaced(ctx.client.clone(), &ns);

    let current_status = build.status.clone().unwrap_or_default();
    let mut new_status = current_status.clone();

    if new_status.observed_generation == build.metadata.generation {
        // Nothing has changed in the spec, just check job status if it exists
        if let Some(job_name) = &new_status.job_name {
            if let Ok(job) = jobs.get(job_name).await {
                update_status_from_job(&mut new_status, &job);

                if current_status.needs_update(&new_status) {
                    update_build_status(&builds, &build, new_status).await?;
                }

                if let Some(status) = job.status {
                    if status.active.unwrap_or(0) > 0 {
                        return Ok(Action::requeue(Duration::from_secs(30)));
                    }
                }
            }
        }
        return Ok(Action::requeue(Duration::from_secs(300))); // Long requeue for up-to-date resources
    }

    let job_name = format!("nixbuild-{}", build.name_any());
    new_status.job_name = Some(job_name.clone());
    new_status.phase = "Building".into();
    new_status.message = Some("Creating build job".into());
    new_status.observed_generation = build.metadata.generation;
    new_status.set_condition("Ready", "False", "BuildStarting", "Creating new build job");

    // does this job exist?
    match jobs.get(&job_name).await {
        Ok(_) => {
            // Job exists but spec changed - we should recreate it
            jobs.delete(&job_name, &Default::default()).await?;
            update_build_status(&builds, &build, new_status).await?;
            return Ok(Action::requeue(Duration::from_secs(5)));
        }
        Err(_) => {
            // make an ew job
            let owner_reference = build.controller_owner_ref(&()).unwrap();
            let job = create_build_job(&build, job_name, owner_reference)?;
            jobs.create(&Default::default(), &job).await?;
            update_build_status(&builds, &build, new_status).await?;
            return Ok(Action::requeue(Duration::from_secs(10))); // Quick requeue to check job status
        }
    }
}

fn update_status_from_job(status: &mut NixBuildStatus, job: &Job) {
    if let Some(job_status) = &job.status {
        if let Some(succeeded) = job_status.succeeded {
            if succeeded > 0 {
                status.phase = "Completed".into();
                status.message = Some("Build completed successfully".into());
                status.set_condition(
                    "Ready",
                    "True",
                    "BuildSucceeded",
                    "Build job completed successfully",
                );
                return;
            }
        }
        if let Some(failed) = job_status.failed {
            if failed > 0 {
                status.phase = "Failed".into();
                status.message = Some("Build job failed".into());
                status.set_condition(
                    "Ready",
                    "False",
                    "BuildFailed",
                    "Build job failed to complete",
                );
                return;
            }
        }
        if let Some(active) = job_status.active {
            if active > 0 {
                status.phase = "Building".into();
                status.message = Some("Build in progress".into());
                status.set_condition("Ready", "False", "Building", "Build job is running");
            }
        }
    }
}

async fn update_build_status(
    builds: &Api<NixBuild>,
    build: &NixBuild,
    status: NixBuildStatus,
) -> Result<(), Error> {
    let status_patch = serde_json::json!({
        "apiVersion": "build.example.com/v1alpha1",
        "kind": "NixBuild",
        "status": status
    });

    builds
        .patch_status(
            &build.name_any(),
            &kube::api::PatchParams::default(),
            &kube::api::Patch::Merge(&status_patch),
        )
        .await?;

    Ok(())
}

fn create_build_job(
    build: &NixBuild,
    name: String,
    owner_reference: OwnerReference,
) -> Result<Job, Error> {
    let container = Container {
        name: "builder".to_string(),
        image: Some("nixos/nix:latest".to_string()),
        command: Some(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            format!(
                r#"
                git clone {} /workspace
                cd /workspace
                {}
                nix --extra-experimental-features nix-command --extra-experimental-features flakes build .#{}
                "#,
                build.spec.git_repo,
                build
                    .spec
                    .git_ref
                    .as_ref()
                    .map(|r| format!("git checkout {}", r))
                    .unwrap_or_default(),
                build
                    .spec
                    .nix_attr
                    .as_ref()
                    .unwrap_or(&"docker".to_string())
            ),
        ]),
        ..Container::default()
    };

    let job = Job {
        metadata: ObjectMeta {
            name: Some(name),
            owner_references: Some(vec![owner_reference]),
            ..ObjectMeta::default()
        },
        spec: Some(JobSpec {
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    containers: vec![container],
                    restart_policy: Some("Never".to_string()),
                    ..PodSpec::default()
                }),
                ..PodTemplateSpec::default()
            },
            ..JobSpec::default()
        }),
        ..Job::default()
    };

    Ok(job)
}

fn error_policy(_resource: Arc<NixBuild>, _error: &Error, _ctx: Arc<ContextData>) -> Action {
    Action::requeue(Duration::from_secs(60))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    tracing::info!("SA token path exists: {}", std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/token").exists());

    tracing::info!("Starting NixBuild controller");

    let client = match Client::try_default().await {
        Ok(c) => {
            tracing::info!("Successfully created Kubernetes client");
            c
        },
        Err(e) => {
            tracing::error!("Failed to create Kubernetes client: {}", e);
            return Err(e.into());
        }
    };

    tracing::info!("Successfully created Kubernetes client");

    let context = Arc::new(ContextData { client: client.clone() });

    let builds: Api<NixBuild> = Api::all(client);
    tracing::info!("Watching NixBuild resources across all namespaces");

    Controller::new(builds, Default::default())
        .run(reconcile, error_policy, context)
        .for_each(|_| futures::future::ready(()))
        .await;

    tracing::info!("Controller shutdown complete");
    Ok(())
}
