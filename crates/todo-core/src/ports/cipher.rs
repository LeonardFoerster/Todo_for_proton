//! `Cipher` und `KeyStore`: Verschlüsselung der Sync-Daten mit einem App-eigenen Schlüssel
//! zusätzlich zur E2E-Verschlüsselung des Backends (ADR-006, `interfaces.md` §5).

use std::fmt;

use super::{MaybeSend, MaybeSync};
use crate::CoreResult;

/// Geheimes Schlüsselmaterial. `Debug` zeigt den Inhalt nie.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretBytes(<{} bytes redacted>)", self.0.len())
    }
}

/// Symmetrische AEAD-Verschlüsselung. Synchron: reine CPU-Arbeit auf kleinen Payloads.
pub trait Cipher: MaybeSend + MaybeSync {
    /// Erzeugt einen neuen, zufälligen Schlüssel passender Länge.
    ///
    /// # Errors
    /// `CoreError::Internal`, wenn keine Zufallsquelle verfügbar ist.
    fn generate_key(&self) -> CoreResult<SecretBytes>;

    /// Verschlüsselt und authentisiert `plaintext` samt `aad`. Ergebnis: `nonce ‖ ciphertext ‖ tag`.
    ///
    /// # Errors
    /// `CoreError::Validation` bei ungeeignetem Schlüssel; `Internal` ohne Zufallsquelle.
    fn seal(&self, key: &SecretBytes, plaintext: &[u8], aad: &[u8]) -> CoreResult<Vec<u8>>;

    /// Entschlüsselt; scheitert, wenn Daten, `aad` oder Schlüssel nicht passen.
    ///
    /// # Errors
    /// `CoreError::Validation` bei Manipulation, falschem `aad` oder falschem Schlüssel.
    fn open(&self, key: &SecretBytes, sealed: &[u8], aad: &[u8]) -> CoreResult<Vec<u8>>;
}

/// Geschützte Ablage für Geheimnisse (libsecret, WebCrypto, Android Keystore).
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait KeyStore: MaybeSend + MaybeSync {
    async fn get(&self, name: &str) -> CoreResult<Option<SecretBytes>>;
    async fn put(&self, name: &str, value: SecretBytes) -> CoreResult<()>;
    /// Fehlt der Eintrag, ist das kein Fehler.
    async fn delete(&self, name: &str) -> CoreResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_secret() {
        let secret = SecretBytes::new(vec![0x41; 32]);
        let shown = format!("{secret:?}");
        assert!(!shown.contains("65") && !shown.contains('A'), "{shown}");
        assert!(shown.contains("32 bytes"));
        assert_eq!(secret.len(), 32);
        assert!(!secret.is_empty());
        assert_eq!(secret.expose()[0], 0x41);
    }
}
