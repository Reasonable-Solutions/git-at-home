use futures::StreamExt;
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{Container, PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::{
    runtime::controller::{Action, Controller},
    Api, Client, Resource, ResourceExt,
};
use std::sync::Arc;
use thiserror::Error;
use tokio::time::Duration;

use build_controller::*;

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

    let builds_list = builds.list(&Default::default()).await?;
    let jobs_list = jobs.list(&Default::default()).await?;

    tracing::info!(
        // How pods is this shitting out?
        "Reconciling, we currently have {} builds and {} jobs",
        builds_list.items.len(),
        jobs_list.items.len()
    );

    if new_status.phase == "Completed" || new_status.phase == "Failed" {
        tracing::info!(
            "Build {} is already {}, nothing to do",
            build.name_any(),
            new_status.phase
        );
        return Ok(Action::requeue(Duration::from_secs(300)));
    }

    if new_status.observed_generation == build.metadata.generation {
        tracing::info!("We've seen this guy before, generations match");
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
        // Hmm Something is very fucky here. 6105 pods??
        return Ok(Action::requeue(Duration::from_secs(300)));
    }

    let job_name = format!("nixbuild-{}", build.name_any());

    new_status.job_name = Some(job_name.clone());
    new_status.phase = "Building".into();
    new_status.message = Some("Creating build job".into());
    new_status.observed_generation = build.metadata.generation;
    tracing::info!("setting condition");
    new_status.set_condition("Ready", "False", "BuildStarting", "Creating new build job");

    match jobs.get(&job_name).await {
        Ok(j) => {
            tracing::info!("Job exists! {}", &job_name);
            if let Some(status) = &j.status {
                update_status_from_job(&mut new_status, &j);

                if current_status.needs_update(&new_status) {
                    update_build_status(&builds, &build, new_status).await?;
                }

                if status.active.unwrap_or(0) > 0 {
                    return Ok(Action::requeue(Duration::from_secs(30)));
                } else {
                    return Ok(Action::requeue(Duration::from_secs(300)));
                }
            }
            Ok(Action::requeue(Duration::from_secs(30)))
        }
        Err(e) => {
            // clearly sufficient?
            if new_status.phase != "Completed" && new_status.phase != "Failed" {
                let owner_reference = build.controller_owner_ref(&()).unwrap();
                tracing::info!(
                    "Creating a new job: {} owner: {:?}",
                    &job_name,
                    &owner_reference
                );
                let job = create_build_job(&build, job_name, owner_reference)?;
                jobs.create(&Default::default(), &job).await?;
                update_build_status(&builds, &build, new_status).await?;
            }
            Ok(Action::requeue(Duration::from_secs(10)))
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
    tracing::info!("update status: {:?}", &build.metadata.name);
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
        image: Some("nix-builder:I".to_string()),
        image_pull_policy: Some("Never".to_owned()),
        // TODO: this shouldn't be a string ffs.
        // TODO: THis needs to be a rootless container, in real life applications
        // TODO: push-to-cache.sh should not be defined here either.
        // TODO: post-build-hooks are blocking, there should be a "put-out-path-on-queue" machine called in the hook
        //       and a separate worker for actually pushing build results.
        command: Some(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            format!(
                r#"
                echo '#!/usr/bin/env bash' >> /home/nixuser/push-to-cache.sh
                echo '/home/nixuser/.nix-profile/bin/nix --extra-experimental-features nix-command --extra-experimental-features flakes copy --to http://{} $OUT_PATHS' >> /home/nixuser/push-to-cache.sh
                chmod +x /home/nixuser/push-to-cache.sh
                which nix
                git clone {} workspace
                cd workspace
                {}
                nix --extra-experimental-features nix-command --extra-experimental-features flakes \
                    --option require-sigs false \
                    --option substitute true \
                    --option extra-substituters http://{} \
                    build .#{} \
                    --post-build-hook /home/nixuser/push-to-cache.sh
                    "#,
                "nix-serve.default.svc.cluster.local:3000",
                build.spec.git_repo,
                build
                    .spec
                    .git_ref
                    .as_ref()
                    .map(|r| format!("git checkout {}", r))
                    .unwrap_or_default(),
                "nix-serve.default.svc.cluster.local:3000",
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

fn error_policy(_resource: Arc<NixBuild>, error: &Error, _ctx: Arc<ContextData>) -> Action {
    tracing::error!("did an error {:?}", error);
    Action::requeue(Duration::from_secs(60))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    tracing::info!(
        "SA token path exists: {}",
        std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/token").exists()
    );

    tracing::info!("Starting NixBuild controller");

    let client = match Client::try_default().await {
        Ok(c) => {
            tracing::info!("Successfully created Kubernetes client");
            c
        }
        Err(e) => {
            tracing::error!("Failed to create Kubernetes client: {}", e);
            return Err(e.into());
        }
    };

    tracing::info!("Successfully created Kubernetes client");

    let context = Arc::new(ContextData {
        client: client.clone(),
    });

    let builds: Api<NixBuild> = Api::all(client);
    tracing::info!("Watching NixBuild resources across all namespaces");

    Controller::new(builds, Default::default())
        .run(reconcile, error_policy, context)
        .for_each(|_| futures::future::ready(()))
        .await;

    tracing::info!("Controller shutdown complete");
    Ok(())
}
