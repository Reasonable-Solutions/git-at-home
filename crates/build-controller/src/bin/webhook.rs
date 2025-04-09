use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use build_controller::*;
use bytes::Bytes;
use futures::io::{AsyncBufReadExt, BufReader};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use kube::{
    api::{Api, ListParams, LogParams},
    Client,
};
use serde::Deserialize;
use std::convert::Infallible;
use std::env;
use tracing::{self, Level};

async fn status(
    State(client): State<Client>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let builds: Api<NixBuild> = Api::namespaced(client, "nixbuilder");

    let build = builds.get(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get build: {e}"),
        )
    })?;

    let status = match &build.status {
        Some(status) => status,
        None => {
            return Ok((
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                serde_json::to_string(&serde_json::json!({
                    "id": id,
                    "status": "pending",
                    "message": "Build status not yet available",
                    "conditions": []
                }))
                .unwrap(),
            ))
        }
    };

    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&serde_json::json!({
            "id": id,
            "status": status.phase,
            "message": status.message,
            "job_name": status.job_name,
            "conditions": status.conditions,
            "observed_generation": status.observed_generation,
            "last_transition_time": status.last_transition_time,
            "creation_timestamp": build.metadata.creation_timestamp,
            "resource_version": build.metadata.resource_version
        }))
        .unwrap(),
    ))
}

pub async fn stream_logs(
    State(client): State<Client>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let pods: Api<Pod> = Api::namespaced(client, "nixbuilder");

    let pod_name = pods
        .list(&ListParams::default().labels(&format!("job-name=nixbuild-build-{id}")))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to list pods: {e}"),
            )
        })?
        .items
        .into_iter()
        .next()
        .ok_or((StatusCode::NOT_FOUND, "no pod found for job".to_string()))?
        .name_any();

    let reader = pods
        .log_stream(
            &pod_name,
            &LogParams {
                follow: true,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to get logs: {e}"),
            )
        })?;
    // wtf
    let lines = BufReader::new(reader).lines();

    let body = Body::from_stream(lines.map(|line| {
        let line = line.unwrap_or_else(|_| "[log error]".to_string());
        Ok::<_, Infallible>(Bytes::from(line + "\n"))
    }));

    Ok(axum::response::Response::builder()
        .header("Content-Type", "text/plain")
        .body(body)
        .unwrap())
}

#[derive(Debug, Deserialize)]
struct BuildRequest {
    git_repo: String,
    git_ref: Option<String>,
    nix_attr: Option<String>,
    image_name: String,
}

async fn handle_build(
    State(client): State<Client>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<BuildRequest>,
) -> Result<String, (StatusCode, String)> {
    let expected = env::var("WEBHOOK_SECRET")
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "missing secret".into()))?;

    let got = headers
        .get("x-webhook-token")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing token".into()))?;

    if got != expected {
        return Err((StatusCode::UNAUTHORIZED, "invalid token".into()));
    }

    let builds: Api<NixBuild> = Api::namespaced(client.clone(), "nixbuilder");
    let build = NixBuild {
        metadata: ObjectMeta {
            generate_name: Some("build-".into()),
            ..Default::default()
        },
        spec: NixBuildSpec {
            git_repo: payload.git_repo,
            git_ref: payload.git_ref,
            nix_attr: payload.nix_attr,
            image_name: payload.image_name,
        },
        status: None,
    };

    match builds.create(&Default::default(), &build).await {
        Ok(b) => Ok(b.name_any()),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to create build: {}", e),
        )),
    }
}

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    tracing::info!("starting webhook");

    let client = Client::try_default()
        .await
        .expect("failed to create kube client");

    let app = Router::new()
        .route("/trigger-build", post(handle_build))
        .route("/logs/:id", get(stream_logs))
        .route("/status/:id", get(status))
        .with_state(client);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind port");
    axum::serve(listener, app).await.unwrap();
}
