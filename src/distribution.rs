use std::str::FromStr;

use crate::settings::UrlOrPath;

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Distribution {
    Nix,
    DeterminateNix,
}

impl Distribution {
    pub fn is_determinate(&self) -> bool {
        self == &Self::DeterminateNix
    }
    pub fn is_upstream(&self) -> bool {
        self == &Self::Nix
    }

    pub fn tarball_location_or(&self, user_preference: &Option<UrlOrPath>) -> TarballLocation {
        if let Some(pref) = user_preference {
            return TarballLocation::UrlOrPath(pref.clone());
        }

        self.tarball_location()
    }

    pub fn tarball_location(&self) -> TarballLocation {
        match self {
            Distribution::Nix => TarballLocation::UrlOrPath(
                UrlOrPath::from_str(NIX_TARBALL_URL)
                    .expect("Fault: the built-in Nix tarball URL does not parse."),
            ),
            Distribution::DeterminateNix => {
                TarballLocation::InMemory(DETERMINATE_NIX_TARBALL_PATH, DETERMINATE_NIX_TARBALL)
            },
        }
    }
}

pub enum TarballLocation {
    UrlOrPath(UrlOrPath),
    InMemory(&'static str, &'static [u8]),
}

pub const NIX_TARBALL_URL: &str = env!("NIX_TARBALL_URL");

pub const DETERMINATE_NIX_TARBALL_PATH: &str = env!("DETERMINATE_NIX_TARBALL_PATH");
/// The DETERMINATE_NIX_TARBALL environment variable should point to a target-appropriate
/// Determinate Nix installation tarball, like determinate-nix-2.31.1-aarch64-darwin.tar.xz.
/// The contents are embedded in the resulting binary.
pub const DETERMINATE_NIX_TARBALL: &[u8] = include_bytes!(env!("DETERMINATE_NIX_TARBALL_PATH"));

/// The DETERMINATE_NIXD_BINARY_PATH environment variable should point to a target-appropriate
/// static build of the Determinate Nixd binary. The contents are embedded in the resulting
/// binary.
pub const DETERMINATE_NIXD_BINARY: &[u8] = include_bytes!(env!("DETERMINATE_NIXD_BINARY_PATH"));
