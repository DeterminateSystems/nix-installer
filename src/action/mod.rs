pub mod base;
pub mod common;
pub mod darwin;
pub mod linux;

use serde::{Deserialize, Serialize};

#[async_trait::async_trait]
#[typetag::serde(tag = "action")]
pub trait Action: Send + Sync + std::fmt::Debug + dyn_clone::DynClone {
    fn describe_execute(&self) -> Vec<ActionDescription>;
    fn describe_revert(&self) -> Vec<ActionDescription>;

    // They should also have an `async fn plan(args...) -> Result<ActionState<Self>, Box<dyn std::error::Error + Send + Sync>>;`
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn action_state(&self) -> ActionState;
}

dyn_clone::clone_trait_object!(Action);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy)]
pub enum ActionState {
    Completed,
    // Only applicable to meta-actions that start multiple sub-actions.
    Progress,
    Uncompleted,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]

pub struct ActionDescription {
    pub description: String,
    pub explanation: Vec<String>,
}

impl ActionDescription {
    fn new(description: String, explanation: Vec<String>) -> Self {
        Self {
            description,
            explanation,
        }
    }
}
