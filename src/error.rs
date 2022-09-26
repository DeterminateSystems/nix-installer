use std::path::PathBuf;

use crate::actions::ActionError;

#[derive(thiserror::Error, Debug)]
pub enum HarmonicError {
    #[error("Error executing action")]
    ActionError(
        #[source]
        #[from]
        ActionError,
    ),
    #[error("Recording install receipt")]
    RecordingReceipt(PathBuf, #[source] std::io::Error),
    #[error(transparent)]
    SerializingReceipt(serde_json::Error),
}
