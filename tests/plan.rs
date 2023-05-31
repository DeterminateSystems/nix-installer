use nix_installer::InstallPlan;

#[cfg(target_os = "linux")]
const LINUX: &str = include_str!("./fixtures/linux/linux.json");
#[cfg(target_os = "linux")]
const STEAM_DECK: &str = include_str!("./fixtures/linux/steam-deck.json");
#[cfg(target_os = "macos")]
const MACOS: &str = include_str!("./fixtures/macos/macos.json");

// Ensure existing plans still parse
// If this breaks and you need to update the fixture, disable these tests, bump `nix_installer` to a new version, and update the plans.
#[cfg(target_os = "linux")]
#[test]
fn plan_compat_linux() -> eyre::Result<()> {
    let _: InstallPlan = serde_json::from_str(LINUX)?;
    Ok(())
}

// Ensure existing plans still parse
// If this breaks and you need to update the fixture, disable these tests, bump `nix_installer` to a new version, and update the plans.
#[cfg(target_os = "linux")]
#[test]
fn plan_compat_steam_deck() -> eyre::Result<()> {
    let _: InstallPlan = serde_json::from_str(STEAM_DECK)?;
    Ok(())
}

// // Ensure existing plans still parse
// // If this breaks and you need to update the fixture, disable these tests, bump `nix_installer` to a new version, and update the plans.
// #[cfg(target_os = "macos")]
// #[test]
// fn plan_compat_macos() -> eyre::Result<()> {
//     let _: InstallPlan = serde_json::from_str(MACOS)?;
//     Ok(())
// }
