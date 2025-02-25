use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

use super::NixCommandExt;
use super::NixEnv;
use super::NixEnvError;

async fn should_skip() -> bool {
    let cmdret = tokio::process::Command::new("nix")
        .set_nix_options(Path::new("/dev/null"))
        .unwrap()
        .arg("--version")
        .output()
        .await;

    if cmdret.is_ok() {
        return false;
    } else {
        println!("Skipping this test because nix isn't in PATH");
        return true;
    }
}

async fn sample_tree(dirname: &str, filename: &str, content: &str) -> Result<PathBuf, NixEnvError> {
    let temp_dir = tempfile::tempdir().map_err(NixEnvError::CreateTempDir)?;

    let sub_dir = temp_dir.path().join(dirname);
    tokio::fs::create_dir(&sub_dir)
        .await
        .map_err(NixEnvError::CreateTempDir)?;

    let file = sub_dir.join(filename);

    let mut f = tokio::fs::File::options()
        .create(true)
        .write(true)
        .open(&file)
        .await
        .map_err(|e| NixEnvError::CreateTempFile(file.to_path_buf(), e))?;

    f.write_all(content.as_bytes())
        .await
        .map_err(|e| NixEnvError::Write(file.to_path_buf(), e))?;

    let mut cmdret = tokio::process::Command::new("nix")
        .set_nix_options(Path::new("/dev/null"))
        .unwrap()
        .args(&["store", "add"])
        .arg(&sub_dir)
        .output()
        .await
        .map_err(|e| NixEnvError::StartNixCommand("nix store add".to_string(), e))?;

    if !cmdret.status.success() {
        return Err(NixEnvError::NixCommand("nix store add".to_string(), cmdret));
    }

    if cmdret.stdout.last() == Some(&b'\n') {
        cmdret.stdout.remove(cmdret.stdout.len() - 1);
    }

    let p = PathBuf::from(std::ffi::OsString::from_vec(cmdret.stdout));

    if p.exists() {
        Ok(p)
    } else {
        Err(NixEnvError::AddPathFailed(std::ffi::OsString::from_vec(
            cmdret.stderr,
        )))
    }
}

#[tokio::test]
async fn test_detect_intersection() {
    if should_skip().await {
        return;
    }

    let profile = tempfile::tempdir().unwrap();
    let profile_path = profile.path().join("profile");

    let tree_1 = sample_tree("foo", "foo", "a").await.unwrap();
    let tree_2 = sample_tree("bar", "foo", "b").await.unwrap();

    (NixEnv {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_1, &tree_2],
    })
    .install_packages()
    .await
    .unwrap_err();
}

#[tokio::test]
async fn test_no_intersection() {
    if should_skip().await {
        return;
    }

    let profile = tempfile::tempdir().unwrap();
    let profile_path = profile.path().join("profile");

    let tree_1 = sample_tree("foo", "foo", "a").await.unwrap();
    let tree_2 = sample_tree("bar", "bar", "b").await.unwrap();

    (NixEnv {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_1, &tree_2],
    })
    .install_packages()
    .await
    .unwrap();

    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("foo"))
            .await
            .unwrap(),
        "a"
    );
    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("bar"))
            .await
            .unwrap(),
        "b"
    );

    let tree_3 = sample_tree("baz", "baz", "c").await.unwrap();
    let tree_4 = sample_tree("tux", "tux", "d").await.unwrap();

    (NixEnv {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_3, &tree_4],
    })
    .install_packages()
    .await
    .unwrap();

    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("baz"))
            .await
            .unwrap(),
        "c"
    );
    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("tux"))
            .await
            .unwrap(),
        "d"
    );
}

#[tokio::test]
async fn test_overlap_replaces() {
    if should_skip().await {
        return;
    }

    let profile = tempfile::tempdir().unwrap();
    let profile_path = profile.path().join("profile");

    let tree_base = sample_tree("fizz", "fizz", "fizz").await.unwrap();
    let tree_1 = sample_tree("foo", "foo", "a").await.unwrap();
    (NixEnv {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_base, &tree_1],
    })
    .install_packages()
    .await
    .unwrap();

    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("fizz"))
            .await
            .unwrap(),
        "fizz"
    );
    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("foo"))
            .await
            .unwrap(),
        "a"
    );

    let tree_2 = sample_tree("foo", "foo", "b").await.unwrap();
    (NixEnv {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_2],
    })
    .install_packages()
    .await
    .unwrap();

    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("foo"))
            .await
            .unwrap(),
        "b"
    );

    let tree_3 = sample_tree("bar", "foo", "c").await.unwrap();
    (NixEnv {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_3],
    })
    .install_packages()
    .await
    .unwrap();

    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("foo"))
            .await
            .unwrap(),
        "c"
    );

    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("fizz"))
            .await
            .unwrap(),
        "fizz"
    );
}
