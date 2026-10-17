//! Domain-Modell, Use-Cases und Ports (Traits) der Task-App.
//!
//! Regel: Dieses Crate kennt weder IO noch Netzwerk, UI, Tauri, WASM oder Proton.
//! Siehe `development/architecture.md` und `development/interfaces.md`.

pub mod app;
pub mod domain;
pub mod error;
pub mod ports;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use error::{CoreError, CoreResult};

#[cfg(test)]
mod tests {
    #[test]
    fn it_compiles() {}
}
