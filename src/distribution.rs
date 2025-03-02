#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Distribution {
    Nix,
    Determinate,
}
