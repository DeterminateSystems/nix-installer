//! Planners for Linux based systems

mod multi;
mod steam_deck;

pub use multi::LinuxMulti;
pub use steam_deck::SteamDeck;
