//! Login per Browser-Session-Fork und Sitzungsverwaltung (`AuthProvider`).

mod fork;
mod provider;
pub(crate) mod session;
#[cfg(test)]
mod tests;

pub use provider::ProtonAuthProvider;
