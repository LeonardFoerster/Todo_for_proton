//! [`XChaCha20Poly1305Cipher`]: der `Cipher`-Port mit XChaCha20-Poly1305 (ADR-006).
//!
//! 192-Bit-Nonces werden pro Aufruf zufällig erzeugt – bei XChaCha20 sicher ohne Zähler.
//! Format von [`Cipher::seal`]: `nonce (24) ‖ ciphertext ‖ tag (16)`. Schlüssel: 32 Bytes.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use todo_core::{
    ports::{Cipher, SecretBytes},
    CoreError, CoreResult,
};

/// Schlüssellänge in Bytes.
pub const KEY_LEN: usize = 32;
/// Nonce-Länge in Bytes (Präfix jeder versiegelten Nachricht).
pub const NONCE_LEN: usize = 24;
/// Länge des Authentisierungs-Tags.
pub const TAG_LEN: usize = 16;

/// Siehe Crate-Doku. Zustandslos; Schlüssel kommen pro Aufruf.
#[derive(Debug, Default, Clone, Copy)]
pub struct XChaCha20Poly1305Cipher;

impl XChaCha20Poly1305Cipher {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

fn aead(key: &SecretBytes) -> CoreResult<XChaCha20Poly1305> {
    XChaCha20Poly1305::new_from_slice(key.expose()).map_err(|_| {
        CoreError::Validation(format!(
            "cipher key must be {KEY_LEN} bytes, got {}",
            key.len()
        ))
    })
}

fn random(buf: &mut [u8]) -> CoreResult<()> {
    getrandom::fill(buf).map_err(|e| CoreError::Internal(format!("no randomness: {e}")))
}

impl Cipher for XChaCha20Poly1305Cipher {
    fn generate_key(&self) -> CoreResult<SecretBytes> {
        let mut key = vec![0; KEY_LEN];
        random(&mut key)?;
        Ok(SecretBytes::new(key))
    }

    fn seal(&self, key: &SecretBytes, plaintext: &[u8], aad: &[u8]) -> CoreResult<Vec<u8>> {
        let aead = aead(key)?;
        let mut nonce = [0; NONCE_LEN];
        random(&mut nonce)?;
        let ciphertext = aead
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| CoreError::Internal("encryption failed".into()))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn open(&self, key: &SecretBytes, sealed: &[u8], aad: &[u8]) -> CoreResult<Vec<u8>> {
        let aead = aead(key)?;
        if sealed.len() < NONCE_LEN + TAG_LEN {
            return Err(CoreError::Validation("sealed data too short".into()));
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
        aead.decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| {
            CoreError::Validation("decryption failed (tampered data, wrong key or aad)".into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (XChaCha20Poly1305Cipher, SecretBytes) {
        let cipher = XChaCha20Poly1305Cipher::new();
        let key = cipher.generate_key().unwrap();
        (cipher, key)
    }

    #[test]
    fn roundtrip_with_overhead_of_nonce_and_tag() {
        let (cipher, key) = setup();
        let sealed = cipher.seal(&key, b"geheime Ops", b"aad").unwrap();
        assert_eq!(sealed.len(), NONCE_LEN + 11 + TAG_LEN);
        assert_eq!(cipher.open(&key, &sealed, b"aad").unwrap(), b"geheime Ops");
    }

    #[test]
    fn nonces_and_keys_are_random() {
        let (cipher, key) = setup();
        assert_eq!(key.len(), KEY_LEN);
        assert_ne!(key, cipher.generate_key().unwrap());
        let a = cipher.seal(&key, b"x", b"").unwrap();
        let b = cipher.seal(&key, b"x", b"").unwrap();
        assert_ne!(a, b, "gleiche Nachricht, verschiedene Nonces");
    }

    #[test]
    fn any_flipped_byte_is_detected() {
        let (cipher, key) = setup();
        let sealed = cipher.seal(&key, b"payload", b"aad").unwrap();
        for i in 0..sealed.len() {
            let mut tampered = sealed.clone();
            tampered[i] ^= 0x01;
            assert!(
                matches!(
                    cipher.open(&key, &tampered, b"aad"),
                    Err(CoreError::Validation(_))
                ),
                "Byte {i}"
            );
        }
    }

    #[test]
    fn wrong_aad_or_key_fails() {
        let (cipher, key) = setup();
        let sealed = cipher.seal(&key, b"payload", b"device-a|1-5").unwrap();
        assert!(cipher.open(&key, &sealed, b"device-b|1-5").is_err());
        let other = cipher.generate_key().unwrap();
        assert!(cipher.open(&other, &sealed, b"device-a|1-5").is_err());
        assert!(cipher.open(&key, &sealed[..10], b"").is_err(), "zu kurz");
    }

    #[test]
    fn rejects_keys_of_wrong_length() {
        let cipher = XChaCha20Poly1305Cipher::new();
        let short = SecretBytes::new(vec![0; 16]);
        assert!(matches!(
            cipher.seal(&short, b"x", b""),
            Err(CoreError::Validation(_))
        ));
    }
}
