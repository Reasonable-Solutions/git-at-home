use futures::StreamExt;
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{
    Container, EnvVar, EnvVarSource, PodSpec, PodTemplateSpec, ResourceRequirements,
    SecretKeySelector,
};
use k8s_openapi::api::core::v1::{LocalObjectReference, VolumeMount};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use k8s_openapi::chrono::Utc;
use kube::{
    runtime::controller::{Action, Controller},
    Api, Client, Resource, ResourceExt,
};
use std::collections::BTreeMap;
use std::env;
use std::sync::Arc;
use thiserror::Error;
use tokio::task;
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

    if new_status.phase == "Completed"
        || new_status.phase == "Failed"
        || new_status.phase == "Deployed"
    {
        tracing::info!(
            "Build {} is already {}, nothing to do",
            build.name_any(),
            new_status.phase
        );

        return Ok(Action::await_change());
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
            if new_status.phase == "Completed"
                || new_status.phase == "Failed"
                || new_status.phase == "Deployed"
            {
                let owner_reference = build.controller_owner_ref(&()).unwrap();
                tracing::info!(
                    "in terminal state, job: {} owner: {:?}",
                    &job_name,
                    &owner_reference
                );
                return Ok(Action::await_change());
            }
            let owner_reference = build.controller_owner_ref(&()).unwrap();
            tracing::info!(
                "Creating a new job: {} owner: {:?}",
                &job_name,
                &owner_reference
            );
            let job = create_build_job(&build, job_name, owner_reference)?;
            jobs.create(&Default::default(), &job).await?;
            update_build_status(&builds, &build, new_status).await?;

            Ok(Action::requeue(Duration::from_secs(30)))
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
        "apiVersion": "build.fyfaen.as/v1alpha1",
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
    let resources = ResourceRequirements {
        ..ResourceRequirements::default()
    };
    let image = "registry.fyfaen.as/nix-builder:1.0.12";
    let builder = Container {
        name: "builder".to_owned(),
        image: Some(image.to_owned()),
        env: Some(vec![
            EnvVar {
                name: "BUILD_NAME".to_owned(),
                value: Some(name.clone()),
                ..Default::default()
            },
            EnvVar {
                name: "ZOT_USERNAME".to_owned(),
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: "zot-creds".to_owned(),
                        key: "ZOT_USERNAME".to_owned(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            EnvVar {
                name: "ZOT_PASSWORD".to_owned(),
                value_from: Some(EnvVarSource {
                    secret_key_ref: Some(SecretKeySelector {
                        name: "zot-creds".to_owned(),
                        key: "ZOT_PASSWORD".to_owned(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ]),
        command: Some(vec![
            "/bin/bash".to_owned(),
            "-c".to_owned(),
            format!(
                r#"

                BUILD_NAME="$BUILD_NAME"
                BUILD_NAME="${{BUILD_NAME#nixbuild-}}" #lol ffs

                publish_status() {{
                    local status="$1"
                    local message="$2"

                    nats --server nats://nats.nats.svc.cluster.local:4222 pub deploy.status.nixbuilder \
                        '{{ "build_name": "'"$BUILD_NAME"'", "status": "'"$status"'", "message": "'"$message"'", "timestamp": "'"$(date -u +"%Y-%m-%dT%H:%M:%SZ")"'" }}'
                }}

                export PATH="/home/nixuser/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
                export NIX_PATH="/home/nixuser/.nix-defexpr/channels:/nix/var/nix/profiles/per-user/root/channels"
                echo '#!/usr/bin/env bash' >> /home/nixuser/push-to-cache.sh
                echo '/home/nixuser/.nix-profile/bin/nix --extra-experimental-features nix-command --extra-experimental-features flakes copy --to http://{} $OUT_PATHS' >> /home/nixuser/push-to-cache.sh
                chmod +x /home/nixuser/push-to-cache.sh
                set -euo pipefail
                which nix


                publish_status "Building" "Populating cache"
                echo "[builder] starting"
                nix --extra-experimental-features nix-command --extra-experimental-features flakes \
                    --option require-sigs false \
                    --option substitute true \
                    --option extra-substituters http://{} \
                    build .#{} \
                    --post-build-hook /home/nixuser/push-to-cache.sh
                echo "[builder]"

                publish_status "Checking" "running nix flake check"
                echo "[builder] running nix flake check"
                if ! nix flake check; then
                publish_status "Failed" "Nix Flake check failed"
                    echo "[builder] nix flake check failed"
                    exit 1
                fi

                echo "[builder] attempting to build image..."
                if ! nix build .#image -o result; then
                    publish_status "Failed" "image generation failed"
                    echo "[builder] image not defined, skipping"
                    exit 0
                fi

                IMAGE_NAME=$(nix eval .#image.imageName --raw | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9_.-/:' '-' | sed 's/^-*//;s/-*$//' )
                IMAGE_TAG=$(nix eval .#image.imageTag --raw | tr -c 'a-zA-Z0-9_.-' '-' | cut -c1-128)
                FULL_TAG="$IMAGE_NAME:$IMAGE_TAG"

                echo "[builder] detected image: $FULL_TAG"

                if [ -z "$ZOT_USERNAME" ] || [ -z "$ZOT_PASSWORD" ]; then
                    publish_status "Failed" "missing push credentials"
                    echo "[builder] missing credentials, cannot push image"
                    exit 1
                fi

                echo "[builder] pushing image to registry" # Move this out into a separate job or like an on-the-fly image realizer
                if ! skopeo copy --dest-creds "$ZOT_USERNAME:$ZOT_PASSWORD" docker-archive:result docker://$FULL_TAG; then
                    publish_status "Failed" "Skopeo copy failed"
                    echo "[builder] skopeo failed" >&2
                    exit 1
                fi
                unset ZOT_USERNAME ZOT_PASSWORD

                echo "[builder] successfully pushed $FULL_TAG to registry"

                echo "[builder] building manifest"
                nix --extra-experimental-features nix-command --extra-experimental-features flakes \
                    --option require-sigs false \
                    --option substitute true \
                    --option extra-substituters http://nix-serve-nixbuilder.svc.cluster.local:3000 \
                    build .#manifests --out-link manifests \
                    --post-build-hook /home/nixuser/push-to-cache.sh ## TODO: Replace this with a fifo and like 4 workers instead of blocking on curl

                echo "[builder] publishing deploy message"
                MANIFEST_CONTENT=$(cat manifests | base64 -w0)
                nats --server nats://nats.nats.svc.cluster.local:4222 pub deploy.ready '{{
                  "manifestB64": "'"$MANIFEST_CONTENT"'",
                  "build_name": "'"$BUILD_NAME"'",
                  "timestamp": "'"$(date -u +"%Y-%m-%dT%H:%M:%SZ")"'"
                }}'

                publish_status "Deploying" "Build proccess completed successfully "
                "#,
                "nix-serve.nixbuilder.svc.cluster.local:3000",
                "nix-serve.nixbuilder.svc.cluster.local:3000",
                build
                    .spec
                    .nix_attr
                    .as_ref()
                    .unwrap_or(&"default".to_string())
            ),
        ]),
        resources: Some(resources),
        ..Container::default()
    };

    let job = Job {
        metadata: ObjectMeta {
            name: Some(name),
            owner_references: Some(vec![owner_reference]),
            ..ObjectMeta::default()
        },
        spec: Some(JobSpec {
            backoff_limit: Some(0),
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    containers: vec![builder],
                    image_pull_secrets: Some(vec![LocalObjectReference {
                        name: "nix-serve-regcred".to_string(),
                    }]),
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
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
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

    let builds: Api<NixBuild> = Api::<NixBuild>::namespaced(client, "nixbuilder"); // Api::all(client); <- for clusterwide resources.
    tracing::info!("Watching NixBuild resources across all namespaces");

    let nats_client = async_nats::connect("nats://nats.nats.svc.cluster.local:4222")
        .await
        .expect("connect to nats");
    task::spawn(async move {
        let mut sub = nats_client.subscribe("deploy.status.*").await.unwrap();

        // Async move, lol no. make a new client!
        let nats_k8s_client = Client::try_default().await.unwrap();
        let nats_builds: Api<NixBuild> = Api::<NixBuild>::namespaced(nats_k8s_client, "nixbuilder");

        while let Some(msg) = sub.next().await {
            let message = String::from_utf8_lossy(&msg.payload);
            let namespace = msg.subject.split('.').nth(2).expect("namespace exists");
            tracing::info!("Extracted namespace: {}", namespace);

            tracing::info!("Received NATS message: {}", message);

            let status_message: Result<DeployStatusMessage, _> = serde_json::from_str(&message);
            match status_message {
                Ok(msg) => {
                    let new_status = NixBuildStatus {
                        phase: msg.status.clone(),
                        job_name: Some(msg.build_name.clone()),
                        message: Some(msg.message.clone()),
                        conditions: vec![],
                        observed_generation: None,
                        last_transition_time: Some(Utc::now().to_rfc3339()),
                    };

                    match nats_builds.get(&msg.build_name).await {
                        Ok(nix_build) => {
                            if nix_build
                                .status
                                .clone()
                                .expect("status should exist")
                                .needs_update(&new_status)
                            {
                                if let Err(e) =
                                    update_build_status(&nats_builds, &nix_build, new_status).await
                                {
                                    tracing::error!("Failed to update build status: {}", e);
                                }
                            }
                        }
                        Err(e) => tracing::error!("Failed to get build {}: {}", msg.build_name, e),
                    }
                }
                Err(e) => tracing::error!("Failed to deserialize NATS message: {}", e),
            }
        }
    });

    Controller::new(builds, Default::default())
        .run(reconcile, error_policy, context)
        .for_each(|_| futures::future::ready(()))
        .await;

    tracing::info!("Controller shutdown complete");
    Ok(())
}
