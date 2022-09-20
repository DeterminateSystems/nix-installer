use std::path::{Path, PathBuf};

use reqwest::Url;

use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchNix {
    url: Url,
    destination: PathBuf,
}

impl FetchNix {
    pub fn plan(url: Url, destination: PathBuf) -> Self {
        Self { url, destination }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for FetchNix {
    type Receipt = FetchNixReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            url, destination
        } = &self;
        vec![ActionDescription::new(
            format!("Fetch Nix from `{url}`"),
            vec![format!(
                "Fetch a Nix archive and unpack it to `{}`", destination.display()
            )],
        )]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { url, destination } = self;
        Ok(FetchNixReceipt { url, destination })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchNixReceipt {
    url: Url,
    destination: PathBuf,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for FetchNixReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
