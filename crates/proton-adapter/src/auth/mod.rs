//! Login per Browser-Session-Fork und Sitzungsverwaltung (`AuthProvider`).

mod fork;
mod login_page;
mod provider;
pub(crate) mod session;
#[cfg(test)]
mod tests;

pub use login_page::write_login_redirect;
pub use provider::ProtonAuthProvider;
