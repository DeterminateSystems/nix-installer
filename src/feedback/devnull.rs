use super::{Feedback, FeedbackWorker};

#[derive(Clone)]
pub struct DevNull;
impl Feedback for DevNull {
    async fn get_feature_ptr_payload<T: serde::de::DeserializeOwned + Send>(
        &mut self,
        _name: impl Into<String> + core::marker::Send,
    ) -> Option<T> {
        None
    }

    async fn set_planner(
        &mut self,
        _planner: &crate::planner::BuiltinPlanner,
    ) -> Result<(), crate::planner::PlannerError> {
        Ok(())
    }

    async fn planning_failed(&mut self, _error: &crate::error::NixInstallerError) {}

    async fn planning_succeeded(&mut self) {}

    async fn install_cancelled(&mut self) {}

    async fn install_failed(&mut self, _error: &crate::error::NixInstallerError) {}

    async fn self_test_failed(&mut self, _error: &crate::error::NixInstallerError) {}

    async fn install_succeeded(&mut self) {}

    async fn uninstall_cancelled(&mut self) {}

    async fn uninstall_failed(&mut self, _error: &crate::error::NixInstallerError) {}

    async fn uninstall_succeeded(&mut self) {}
}

pub struct DevNullWorker;
impl FeedbackWorker for DevNullWorker {
    async fn submit(self) {}
}
