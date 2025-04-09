use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as b64;
use base64::Engine;
use futures::StreamExt;
use k8s_openapi::serde_json::Value;
use kube::discovery::Discovery;
use kube::{
    api::{DynamicObject, Patch, PatchParams},
    Api, Client,
};
use serde_yaml;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("[deployer] starting...");

    info!("[deployer] initializing rustls crypto provider");
    match rustls::crypto::ring::default_provider().install_default() {
        Ok(_) => info!("[deployer] rustls crypto provider installed"),
        Err(e) => {
            error!(?e, "[deployer] failed to install rustls crypto provider");
            std::process::exit(1);
        }
    }

    info!("[deployer] creating Kubernetes client");
    let client = Client::try_default()
        .await
        .context("failed to create Kubernetes client")?;

    let nats_url = std::env::var("NATS_URL")
        .unwrap_or_else(|_| "nats://nats.nats.svc.cluster.local:4222".to_string());
    info!(%nats_url, "[deployer] connecting to NATS");
    let nc = async_nats::connect(&nats_url)
        .await
        .context("failed to connect to NATS")?;

    info!("[deployer] subscribing to subject 'deploy.ready'");
    let mut sub = nc.subscribe("deploy.ready").await?;
    info!("[deployer] waiting for messages on deploy.ready");

    while let Some(msg) = sub.next().await {
        match handle_message(&client, &msg.payload).await {
            Ok(_) => info!("[deployer] message handled successfully"),
            Err(err) => error!(?err, "[deployer] error handling message"),
        }
    }

    Ok(())
}

async fn handle_message(client: &Client, data: &[u8]) -> Result<()> {
    info!(
        "[deployer] received raw message: {}",
        String::from_utf8_lossy(data)
    );

    let v: Value = serde_json::from_slice(data)?;
    let b64_str = v["manifestB64"]
        .as_str()
        .context("missing manifestB64 field")?;
    let manifest_bytes = b64.decode(b64_str)?;
    let manifest_str = std::str::from_utf8(&manifest_bytes)?;

    info!("[deployer] decoded manifest, applying...");
    apply_from_manifest_str(client, manifest_str).await?;
    Ok(())
}

async fn apply_from_manifest_str(client: &Client, manifest: &str) -> Result<()> {
    let discovery = Discovery::new(client.clone()).run().await?;

    for doc in manifest.split("---") {
        let doc = doc.trim();
        if doc.is_empty() {
            continue;
        }

        let val: Value = serde_yaml::from_str(doc)
            .with_context(|| format!("failed to parse YAML doc:\n{doc}"))?;

        let api_version = val["apiVersion"].as_str().context("missing apiVersion")?;
        let kind = val["kind"].as_str().context("missing kind")?;
        let name = val["metadata"]["name"]
            .as_str()
            .context("missing metadata.name")?;
        let namespace = val["metadata"]["namespace"].as_str().unwrap_or("default");

        info!(%kind, %name, %api_version, %namespace, "[deployer] applying resource");

        let (group, version) = match api_version.split_once('/') {
            Some((g, v)) => (g, v),
            None => ("", api_version),
        };

        let group_match = discovery
            .groups()
            .find(|g| g.name() == group)
            .context("could not find api group")?;

        let (ar, _caps) = group_match
            .recommended_kind(kind)
            .context("could not resolve kind in group")?;

        let api: Api<DynamicObject> = Api::namespaced_with(client.clone(), namespace, &ar);

        api.patch(
            name,
            &PatchParams::apply("nix-build-deployer").force(),
            &Patch::Apply(val.clone()),
        )
        .await
        .context(format!("failed to apply {kind}/{name}"))?;

        info!(%kind, %name, "[deployer] successfully applied");
    }
    Ok(())
}
