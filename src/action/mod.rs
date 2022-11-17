pub mod base;
pub mod common;
pub mod darwin;
pub mod linux;

use serde::{Deserialize, Serialize};

#[async_trait::async_trait]
#[typetag::serde(tag = "action")]
pub trait Action: Send + Sync + std::fmt::Debug + dyn_clone::DynClone {
    fn tracing_synopsis(&self) -> String;
    fn execute_description(&self) -> Vec<ActionDescription>;
    fn revert_description(&self) -> Vec<ActionDescription>;
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn action_state(&self) -> ActionState;
    fn set_action_state(&mut self, new_state: ActionState);

    // They should also have an `async fn plan(args...) -> Result<ActionState<Self>, Box<dyn std::error::Error + Send + Sync>>;`
}

#[async_trait::async_trait]
pub(crate) trait ActionImplementation: Action {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state() == ActionState::Completed {
            return vec![];
        }
        return self.execute_description();
    }
    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state() == ActionState::Uncompleted {
            return vec![];
        }
        return self.revert_description();
    }

    async fn try_execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.action_state() == ActionState::Completed {
            tracing::trace!("Completed: (Already done) {}", self.tracing_synopsis());
            return Ok(());
        }
        self.set_action_state(ActionState::Progress);
        tracing::debug!("Executing: {}", self.tracing_synopsis());
        self.execute().await?;
        self.set_action_state(ActionState::Completed);
        tracing::debug!("Completed: {}", self.tracing_synopsis());
        Ok(())
    }

    async fn try_revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.action_state() == ActionState::Uncompleted {
            tracing::trace!("Reverted: (Already done) {}", self.tracing_synopsis());
            return Ok(());
        }
        self.set_action_state(ActionState::Progress);
        tracing::debug!("Reverting: {}", self.tracing_synopsis());
        self.revert().await?;
        tracing::debug!("Reverted: {}", self.tracing_synopsis());
        self.set_action_state(ActionState::Uncompleted);
        Ok(())
    }
}

impl ActionImplementation for dyn Action {}

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
