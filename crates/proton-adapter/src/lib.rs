//! Proton-Adapter: Proton Drive hinter den Ports `AuthProvider` und `RemoteStore`.
//!
//! **Einziger** Ort für Proton-Code (AGENTS.md §3). Proton-Typen, Endpunkte und Krypto-Details
//! verlassen dieses Crate nie; nach außen gehen nur Port-Typen und [`ApiConfig`].
//!
//! Laufzeit: Die HTTP-Schicht (`reqwest`) braucht einen tokio-Kontext (Tauri, `#[tokio::main]`).

mod api_client;
mod auth;
mod drive;
mod error;
mod secret;

pub use api_client::{ApiConfig, DEFAULT_BASE_URL};
pub use auth::{write_login_redirect, ProtonAuthProvider};
pub use drive::ProtonDriveRemoteStore;
