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

impl NixBuildSpec {
    pub fn new(
        git_repo: String,
        git_ref: Option<String>,
        nix_attr: Option<String>,
        image_name: String,
    ) -> Self {
        Self {
            git_repo,
            git_ref,
            nix_attr,
            image_name,
        }
    }
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
    pub fn needs_update(&self, new_status: &NixBuildStatus) -> bool {
        self.phase != new_status.phase
            || self.job_name != new_status.job_name
            || self.message != new_status.message
            || self.observed_generation != new_status.observed_generation
            || self.conditions.len() != new_status.conditions.len()
    }

    pub fn set_condition(&mut self, type_: &str, status: &str, reason: &str, message: &str) {
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
