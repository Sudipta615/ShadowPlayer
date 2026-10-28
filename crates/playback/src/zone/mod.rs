//! Autonomous audio zones, master-slave synchronisation, and ZoneSwitch routing.

mod coordinator;
mod types;

#[cfg(test)]
mod tests;

pub use coordinator::*;
pub use types::*;
