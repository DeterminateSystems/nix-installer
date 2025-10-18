pub mod client;
pub mod devnull;

pub trait Feedback: Clone + Send + Sync {
    fn set_planner(
        &mut self,
        planner: &crate::planner::BuiltinPlanner,
    ) -> impl std::future::Future<Output = Result<(), crate::planner::PlannerError>> + Send;

    fn get_feature_ptr_payload<
        T: serde::ser::Serialize + serde::de::DeserializeOwned + Send + std::fmt::Debug,
    >(
        &self,
        name: impl Into<String> + Send + std::fmt::Debug,
    ) -> impl std::future::Future<Output = Option<T>> + Send;

    fn planning_failed(
        &mut self,
        error: &crate::error::NixInstallerError,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn planning_succeeded(&mut self) -> impl std::future::Future<Output = ()> + Send;

    fn install_cancelled(&mut self) -> impl std::future::Future<Output = ()> + Send;

    fn install_failed(
        &mut self,
        error: &crate::error::NixInstallerError,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn self_test_failed(
        &mut self,
        error: &crate::error::NixInstallerError,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn install_succeeded(&mut self) -> impl std::future::Future<Output = ()> + Send;

    fn uninstall_cancelled(&mut self) -> impl std::future::Future<Output = ()> + Send;

    fn uninstall_failed(
        &mut self,
        error: &crate::error::NixInstallerError,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn uninstall_succeeded(&mut self) -> impl std::future::Future<Output = ()> + Send;
}

pub trait FeedbackWorker {
    fn submit(self) -> impl std::future::Future<Output = ()> + Send;
}
