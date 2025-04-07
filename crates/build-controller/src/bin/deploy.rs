use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as b64;
use base64::Engine;
use futures::StreamExt;
use k8s_openapi::serde_json::Value;
use kube::core::gvk::GroupVersionKind;
use kube::core::ApiResource;
use kube::{
    api::{DynamicObject, Patch, PatchParams},
    Api, Client,
};
use serde_yaml;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let client = Client::try_default().await?;
    let nc = async_nats::connect("nats://nats.nats.svc.cluster.local:4222").await?;

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

        info!(%kind, %name, %api_version, "[deployer] applying resource");

        let (group, version) = match api_version.split_once('/') {
            Some((g, v)) => (g, v),
            None => ("", api_version),
        };

        let plural = format!("{}s", kind.to_ascii_lowercase());
        let gvk = GroupVersionKind::gvk(group, version, kind);
        let api_resource = ApiResource::from_gvk_with_plural(&gvk, &plural);

        let api: Api<DynamicObject> = Api::all_with(client.clone(), &api_resource);

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
