//! Proton-Adapter: Proton Drive hinter den Ports `AuthProvider` und `RemoteStore`.
//!
//! **Einziger** Ort für Proton-Code (AGENTS.md §3). Proton-Typen, Endpunkte und Krypto-Details
//! verlassen dieses Crate nie; nach außen gehen nur Port-Typen und [`ApiConfig`].
//!
//! Laufzeit: Die HTTP-Schicht (`reqwest`) braucht einen tokio-Kontext (Tauri, `#[tokio::main]`).

// Bis `ProtonAuthProvider`/`ProtonDriveRemoteStore` (P6-03/P6-04) sind die Bausteine ungenutzt.
#[allow(dead_code)]
mod api_client;
#[allow(dead_code)]
mod error;
#[allow(dead_code)]
mod secret;

pub use api_client::{ApiConfig, DEFAULT_BASE_URL};
