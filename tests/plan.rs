use harmonic::InstallPlan;

const LINUX_MULTI: &str = include_str!("./fixtures/linux/linux-multi.json");
const STEAM_DECK: &str = include_str!("./fixtures/linux/steam-deck.json");
const DARWIN_MULTI: &str = include_str!("./fixtures/darwin/darwin-multi.json");

// Ensure existing plans still parse
// If this breaks and you need to update the fixture, disable these tests, cook bump to a new version, and update the plans.
#[test]
fn plan_compat_linux_multi() -> eyre::Result<()> {
    let _: InstallPlan = serde_json::from_str(LINUX_MULTI)?;
    Ok(())
}

// Ensure existing plans still parse
// If this breaks and you need to update the fixture, disable these tests, cook bump to a new version, and update the plans.
#[test]
fn plan_compat_steam_deck() -> eyre::Result<()> {
    let _: InstallPlan = serde_json::from_str(STEAM_DECK)?;
    Ok(())
}

// Ensure existing plans still parse
// If this breaks and you need to update the fixture, disable these tests, cook bump to a new version, and update the plans.
#[test]
fn plan_compat_darwin_multi() -> eyre::Result<()> {
    let _: InstallPlan = serde_json::from_str(DARWIN_MULTI)?;
    Ok(())
}
