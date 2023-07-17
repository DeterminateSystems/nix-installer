use serde::{Deserialize, Serialize};
use tracing::{Instrument, Span};

use super::{Action, ActionDescription, ActionError, ActionTag};

/// A wrapper around an [`Action`](crate::action::Action) which tracks the [`ActionState`] and
/// handles some tracing output
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StatefulAction<A> {
    pub(crate) action: A,
    pub(crate) state: ActionState,
}

impl<A> From<A> for StatefulAction<A>
where
    A: Action,
{
    fn from(action: A) -> Self {
        Self {
            action,
            state: ActionState::Uncompleted,
        }
    }
}

impl StatefulAction<Box<dyn Action>> {
    pub fn inner_typetag_name(&self) -> &'static str {
        self.action.typetag_name()
    }
    pub fn tracing_synopsis(&self) -> String {
        self.action.tracing_synopsis()
    }
    /// A tracing span suitable for the action
    pub fn tracing_span(&self) -> Span {
        self.action.tracing_span()
    }
    /// A description of what this action would do during execution
    pub fn describe_execute(&self) -> Vec<ActionDescription> {
        match self.state {
            ActionState::Completed | ActionState::Skipped => {
                vec![]
            },
            _ => self.action.execute_description(),
        }
    }
    /// A description of what this action would do during revert
    pub fn describe_revert(&self) -> Vec<ActionDescription> {
        match self.state {
            ActionState::Uncompleted | ActionState::Skipped => {
                vec![]
            },
            _ => self.action.revert_description(),
        }
    }
    /// Perform any execution steps
    ///
    /// You should prefer this ([`try_execute`][StatefulAction::try_execute]) over [`execute`][Action::execute] as it handles [`ActionState`] and does tracing
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn try_execute(&mut self) -> Result<(), ActionError> {
        match self.state {
            ActionState::Completed => {
                tracing::trace!(
                    "Completed: (Already done) {}",
                    self.action.tracing_synopsis()
                );
                Ok(())
            },
            ActionState::Skipped => {
                tracing::trace!("Skipped: {}", self.action.tracing_synopsis());
                Ok(())
            },
            _ => {
                self.state = ActionState::Progress;
                tracing::debug!("Executing: {}", self.action.tracing_synopsis());
                self.action.execute().await?;
                self.state = ActionState::Completed;
                tracing::debug!("Completed: {}", self.action.tracing_synopsis());
                Ok(())
            },
        }
    }
    /// Perform any revert steps
    ///
    /// You should prefer this ([`try_revert`][StatefulAction::try_revert]) over [`revert`][Action::revert] as it handles [`ActionState`] and does tracing
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn try_revert(&mut self) -> Result<(), ActionError> {
        match self.state {
            ActionState::Uncompleted => {
                tracing::trace!(
                    "Reverted: (Already done) {}",
                    self.action.tracing_synopsis()
                );
                Ok(())
            },
            ActionState::Skipped => {
                tracing::trace!("Skipped: {}", self.action.tracing_synopsis());
                Ok(())
            },
            _ => {
                self.state = ActionState::Progress;
                tracing::debug!("Reverting: {}", self.action.tracing_synopsis());
                self.action.revert().await?;
                tracing::debug!("Reverted: {}", self.action.tracing_synopsis());
                self.state = ActionState::Uncompleted;
                Ok(())
            },
        }
    }
}

impl<A> StatefulAction<A>
where
    A: Action,
{
    pub fn tag() -> ActionTag {
        A::action_tag()
    }
    pub fn action_tag(&self) -> ActionTag {
        A::action_tag()
    }
    pub fn tracing_synopsis(&self) -> String {
        self.action.tracing_synopsis()
    }

    /// A tracing span suitable for the action
    pub fn tracing_span(&self) -> Span {
        self.action.tracing_span()
    }

    pub fn inner(&self) -> &A {
        &self.action
    }

    pub fn boxed(self) -> StatefulAction<Box<dyn Action>>
    where
        Self: 'static,
    {
        StatefulAction {
            action: Box::new(self.action),
            state: self.state,
        }
    }
    /// A description of what this action would do during execution
    pub fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.state == ActionState::Completed {
            return vec![];
        }
        self.action.execute_description()
    }
    /// A description of what this action would do during revert
    pub fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.state == ActionState::Uncompleted {
            return vec![];
        }
        self.action.revert_description()
    }
    /// Perform any execution steps
    ///
    /// You should prefer this ([`try_execute`][StatefulAction::try_execute]) over [`execute`][Action::execute] as it handles [`ActionState`] and does tracing
    pub async fn try_execute(&mut self) -> Result<(), ActionError> {
        let span = self.action.tracing_span();
        match self.state {
            ActionState::Completed => {
                tracing::trace!(
                    parent: &span,
                    "Completed: (Already done) {}",
                    self.action.tracing_synopsis()
                );
                Ok(())
            },
            ActionState::Skipped => {
                tracing::trace!(parent: &span, "Skipped: {}", self.action.tracing_synopsis());
                Ok(())
            },
            _ => {
                self.state = ActionState::Progress;
                tracing::debug!(
                    parent: &span,
                    "Executing: {}",
                    self.action.tracing_synopsis()
                );
                self.action.execute().instrument(span.clone()).await?;
                self.state = ActionState::Completed;
                tracing::debug!(
                    parent: &span,
                    "Completed: {}",
                    self.action.tracing_synopsis()
                );
                Ok(())
            },
        }
    }
    /// Perform any revert steps
    ///
    /// You should prefer this ([`try_revert`][StatefulAction::try_revert]) over [`revert`][Action::revert] as it handles [`ActionState`] and does tracing
    pub async fn try_revert(&mut self) -> Result<(), ActionError> {
        let span = self.action.tracing_span();
        match self.state {
            ActionState::Uncompleted => {
                tracing::trace!(
                    parent: &span,
                    "Reverted: (Already done) {}",
                    self.action.tracing_synopsis()
                );
                Ok(())
            },
            ActionState::Skipped => {
                tracing::trace!(parent: &span, "Skipped: {}", self.action.tracing_synopsis());
                Ok(())
            },
            _ => {
                self.state = ActionState::Progress;
                tracing::debug!(
                    parent: &span,
                    "Reverting: {}",
                    self.action.tracing_synopsis()
                );
                self.action.revert().instrument(span.clone()).await?;
                tracing::debug!(
                    parent: &span,
                    "Reverted: {}",
                    self.action.tracing_synopsis()
                );
                self.state = ActionState::Uncompleted;
                Ok(())
            },
        }
    }

    pub fn completed(action: A) -> Self {
        Self {
            state: ActionState::Completed,
            action,
        }
    }

    pub fn skipped(action: A) -> Self {
        Self {
            state: ActionState::Skipped,
            action,
        }
    }

    pub fn uncompleted(action: A) -> Self {
        Self {
            state: ActionState::Uncompleted,
            action,
        }
    }
}

/** The state of an [`Action`](crate::action::Action)
*/
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy)]
pub enum ActionState {
    /**
    If [`Completed`](ActionState::Completed) an [`Action`](crate::action::Action) will be skipped
    on [`InstallPlan::install`](crate::InstallPlan::install), and reverted on [`InstallPlan::uninstall`](crate::InstallPlan::uninstall)
    */
    Completed,
    /**
    If [`Progress`](ActionState::Progress) an [`Action`](crate::action::Action) will be run on
    [`InstallPlan::install`](crate::InstallPlan::install) and [`InstallPlan::uninstall`](crate::InstallPlan::uninstall)

    Only applicable to meta-actions that contain other multiple sub-actions.
    */
    Progress,
    /**
    If [`Completed`](ActionState::Completed) an [`Action`](crate::action::Action) will be skipped
    on [`InstallPlan::uninstall`](crate::InstallPlan::uninstall) and executed on [`InstallPlan::install`](crate::InstallPlan::install)
    */
    Uncompleted,
    /**
    If [`Skipped`](ActionState::Skipped) an [`Action`](crate::action::Action) will be skipped
    on [`InstallPlan::install`](crate::InstallPlan::install) and [`InstallPlan::uninstall`](crate::InstallPlan::uninstall)

    Typically this is used by actions which detect they are already completed in their `plan` phase.
    */
    Skipped,
}
