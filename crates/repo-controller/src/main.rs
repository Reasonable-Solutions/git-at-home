use futures_util::StreamExt;
use kube::api::{Patch, PatchParams};
use kube::{CustomResource, ResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use chrono::Utc;
use kube::{
    runtime::controller::{Action, Controller},
    Api, Client,
};

use std::{collections::BTreeMap, sync::Arc};
use tokio::time::Duration;
use tracing::info;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "git.platform.dev",
    version = "v1alpha1",
    kind = "GitUser",
    namespaced
)]
pub struct GitUserSpec {
    pub public_keys: Vec<String>,
}

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "git.platform.dev",
    version = "v1alpha1",
    kind = "GitRepository",
    namespaced,
    status = "GitRepositoryStatus",
    printcolumn = r#"{"name":"Repo","jsonPath":".spec.repo_name","type":"string"}"#,
    printcolumn = r#"{"name":"Owner","jsonPath":".spec.owner","type":"string"}"#
)]
pub struct GitRepositorySpec {
    pub repo_name: String,
    pub owner: String,
    pub visibility: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct GitRepositoryStatus {
    pub message: Option<String>,
    pub observed_generation: Option<i64>,
    pub ready: bool,
    pub latest_commit: Option<String>,
    pub last_updated: Option<String>,
}

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "git.platform.dev",
    version = "v1alpha1",
    kind = "GitAccess",
    namespaced,
    printcolumn = r#"{"name":"Repo","jsonPath":".spec.repo","type":"string"}"#,
    printcolumn = r#"{"name":"User","jsonPath":".spec.user","type":"string"}"#
)]
pub struct GitAccessSpec {
    pub repo: String,
    pub user: String,
    pub permissions: Vec<String>, // e.g., ["read", "write"]
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Kube error: {0}")]
    Kube(#[from] kube::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
struct Context {
    client: Client,
    repo_base: String,
}

async fn reconcile(repo: Arc<GitRepository>, ctx: Arc<Context>) -> Result<Action, Error> {
    let name = repo.name_any();
    let ns = repo.namespace().unwrap();
    let path = format!("{}/{}.git", ctx.repo_base, name);

    if std::fs::metadata(&path).is_err() {
        std::process::Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg(&path)
            .output()?;
    }

    let commits = std::process::Command::new("git")
        .arg("--git-dir")
        .arg(&path)
        .arg("rev-parse")
        .arg("HEAD")
        .output()?;

    let commit_hash = if commits.status.success() {
        Some(String::from_utf8_lossy(&commits.stdout).trim().to_string())
    } else {
        None
    };

    let status = GitRepositoryStatus {
        message: Some("Ready".to_string()),
        observed_generation: repo.metadata.generation,
        ready: true,
        latest_commit: commit_hash,
        last_updated: Some(Utc::now().to_rfc3339()),
    };

    let api: Api<GitRepository> = Api::namespaced(ctx.client.clone(), &ns);

    let patch = Patch::Apply(serde_json::json!({
      "apiVersion": "git.platform.dev/v1alpha1",
      "kind": "GitRepository",
      "status": status,
    }));
    api.patch_status(&name, &PatchParams::apply("repo-controller"), &patch)
        .await?;

    Ok(Action::requeue(Duration::from_secs(600)))
}

fn error_policy(_object: Arc<GitRepository>, _error: &Error, _ctx: Arc<Context>) -> Action {
    Action::requeue(Duration::from_secs(300))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let client = Client::try_default().await?;
    let repo_base = "/git/repos".to_string();
    let context = Arc::new(Context {
        client: client.clone(),
        repo_base,
    });

    let repos = Api::<GitRepository>::all(client.clone());
    Controller::new(repos, Default::default())
        .run(reconcile, error_policy, context)
        .for_each(|res| async move {
            match res {
                Ok(o) => info!("reconciled {:?}", o),
                Err(e) => info!("reconcile failed: {:?}", e),
            }
        })
        .await;

    Ok(())
}
