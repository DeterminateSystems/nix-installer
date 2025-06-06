use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

use super::super::WriteToDefaultProfile;
use super::NixCommandExt;
use super::NixProfile;

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

async fn sample_tree(dirname: &str, filename: &str, content: &str) -> PathBuf {
    let temp_dir = tempfile::tempdir().unwrap();

    let sub_dir = temp_dir.path().join(dirname);
    tokio::fs::create_dir(&sub_dir).await.unwrap();

    let file = sub_dir.join(filename);

    let mut f = tokio::fs::File::options()
        .create(true)
        .write(true)
        .open(&file)
        .await
        .unwrap();

    f.write_all(content.as_bytes()).await.unwrap();

    let mut cmdret = tokio::process::Command::new("nix")
        .set_nix_options(Path::new("/dev/null"))
        .unwrap()
        .args(&["store", "add"])
        .arg(&sub_dir)
        .output()
        .await
        .unwrap();

    assert!(
        cmdret.status.success(),
        "Running nix-store add failed: {:#?}",
        cmdret,
    );

    if cmdret.stdout.last() == Some(&b'\n') {
        cmdret.stdout.remove(cmdret.stdout.len() - 1);
    }

    let p = PathBuf::from(std::ffi::OsString::from_vec(cmdret.stdout));

    assert!(
        p.exists(),
        "Adding a path to the Nix store failed...: {:#?}",
        cmdret.stderr
    );

    p
}

#[tokio::test]
async fn test_detect_intersection() {
    if should_skip().await {
        return;
    }

    let profile = tempfile::tempdir().unwrap();
    let profile_path = profile.path().join("profile");

    let tree_1 = sample_tree("foo", "foo", "a").await;
    let tree_2 = sample_tree("bar", "foo", "b").await;

    (NixProfile {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_1, &tree_2],
    })
    .install_packages(WriteToDefaultProfile::Isolated)
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

    let tree_1 = sample_tree("foo", "foo", "a").await;
    let tree_2 = sample_tree("bar", "bar", "b").await;

    (NixProfile {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_1, &tree_2],
    })
    .install_packages(WriteToDefaultProfile::Isolated)
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

    let tree_3 = sample_tree("baz", "baz", "c").await;
    let tree_4 = sample_tree("tux", "tux", "d").await;

    (NixProfile {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_3, &tree_4],
    })
    .install_packages(WriteToDefaultProfile::Isolated)
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

    let tree_base = sample_tree("fizz", "fizz", "fizz").await;
    let tree_1 = sample_tree("foo", "foo", "a").await;
    (NixProfile {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_base, &tree_1],
    })
    .install_packages(WriteToDefaultProfile::Isolated)
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

    let tree_2 = sample_tree("foo", "foo", "b").await;
    (NixProfile {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_2],
    })
    .install_packages(WriteToDefaultProfile::Isolated)
    .await
    .unwrap();

    assert_eq!(
        tokio::fs::read_to_string(profile_path.join("foo"))
            .await
            .unwrap(),
        "b"
    );

    let tree_3 = sample_tree("bar", "foo", "c").await;
    (NixProfile {
        nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
        nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
        profile: &profile_path,
        pkgs: &[&tree_3],
    })
    .install_packages(WriteToDefaultProfile::Isolated)
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
