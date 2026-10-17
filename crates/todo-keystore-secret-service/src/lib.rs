//! [`KeyStore`](todo_core::ports::KeyStore) auf dem System-Schlüsselbund (freedesktop Secret
//! Service: GNOME Keyring, KWallet/ksecretd, KeePassXC …).
//!
//! Plattform-, nicht anbieterspezifisch: Hier liegen Sync-Schlüssel und Sitzungsdaten beliebiger
//! Adapter. Nur unter Linux verfügbar; braucht einen tokio-Kontext (zbus).

#[cfg(target_os = "linux")]
mod secret_service;

#[cfg(target_os = "linux")]
pub use secret_service::SecretServiceKeyStore;
