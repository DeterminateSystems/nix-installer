use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OnMissing {
    Ignore,
    Error,
}

#[tracing::instrument(skip(path), fields(path = %path.display()))]
pub(crate) async fn remove_file(path: &Path, on_missing: OnMissing) -> std::io::Result<()> {
    tracing::trace!("Removing file");
    let res = tokio::fs::remove_file(path).await;
    match res {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && on_missing == OnMissing::Ignore => {
            tracing::trace!("Ignoring nonexistent file");
            Ok(())
        },
        e @ Err(_) => e,
    }
}

#[tracing::instrument(skip(path), fields(path = %path.display()))]
pub(crate) async fn remove_dir_all(path: &Path, on_missing: OnMissing) -> std::io::Result<()> {
    tracing::trace!("Removing directory and all contents");
    let res = tokio::fs::remove_dir_all(path).await;
    match res {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && on_missing == OnMissing::Ignore => {
            tracing::trace!("Ignoring nonexistent directory");
            Ok(())
        },
        e @ Err(_) => e,
    }
}
