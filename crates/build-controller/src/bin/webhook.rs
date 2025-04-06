use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use build_controller::*;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{Api, Client, ResourceExt};
use serde::Deserialize;
use tracing::{self, Level};

#[derive(Debug, Deserialize)]
struct BuildRequest {
    git_repo: String,
    git_ref: Option<String>,
    nix_attr: Option<String>,
    image_name: String,
}

use std::env;

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
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    tracing::info!("starting webhook");

    let client = Client::try_default()
        .await
        .expect("failed to create kube client");

    let app = Router::new()
        .route("/trigger-build", post(handle_build))
        .with_state(client);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind port");
    axum::serve(listener, app).await.unwrap();
}
