use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum HarmonicError {
    #[error("Error executing action")]
    ActionError(
        #[source]
        #[from]
        Box<dyn std::error::Error + Send + Sync>,
    ),
    #[error("Recording install receipt")]
    RecordingReceipt(PathBuf, #[source] std::io::Error),
    #[error(transparent)]
    SerializingReceipt(serde_json::Error),
    #[error("Cancelled by user")]
    Cancelled,
}
