//! Bausteine des Browser-Session-Forks (wie das offizielle Drive-CLI für Drittanbieter-Forks):
//! Login-URL mit Einmal-Schlüssel im URL-Fragment, Entschlüsselung des `Payload`.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use todo_core::{CoreError, CoreResult};

use crate::secret::Secret;

const ACCOUNT_URL: &str = "https://account.proton.me";
/// Client-ID für Drittanbieter (offizielles CLI: `cli-drive`, Forks: `external-drive`).
const CLIENT_ID: &str = "external-drive";
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const PAYLOAD_AAD: &[u8] = b"fork";

/// 32 Zufallsbytes, die nur im URL-Fragment zum Browser gehen (nie an den Server).
#[derive(Clone)]
pub(crate) struct ForkKey([u8; 32]);

impl ForkKey {
    pub(crate) fn generate() -> CoreResult<Self> {
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|e| CoreError::Internal(format!("random: {e}")))?;
        Ok(Self(key))
    }

    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Seite, auf der der Nutzer den Fork bestätigt.
pub(crate) fn login_url(user_code: &str, key: &ForkKey) -> Secret {
    let payload = format!("0:{user_code}:{}:{CLIENT_ID}", STANDARD.encode(key.0));
    Secret::new(format!(
        "{ACCOUNT_URL}/desktop/login?app=drive&pv=3#payload={}",
        encode_uri_component(&payload)
    ))
}

#[derive(Deserialize)]
struct ForkPayload {
    #[serde(rename = "keyPassword")]
    key_password: String,
}

/// `Payload` = base64(nonce ‖ AES-256-GCM(JSON) ‖ tag), AAD `"fork"` → `keyPassword`.
pub(crate) fn decrypt_payload(key: &ForkKey, payload_b64: &str) -> CoreResult<Secret> {
    let invalid = |what: &str| CoreError::RemoteRejected(format!("fork payload: {what}"));
    let blob = STANDARD
        .decode(payload_b64)
        .map_err(|_| invalid("not base64"))?;
    if blob.len() < NONCE_LEN + TAG_LEN {
        return Err(invalid("too short"));
    }
    let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(&key.0).map_err(|_| invalid("key length"))?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: PAYLOAD_AAD,
            },
        )
        .map_err(|_| invalid("decryption failed"))?;
    let parsed: ForkPayload =
        serde_json::from_slice(&plain).map_err(|_| invalid("no keyPassword"))?;
    Ok(Secret::new(parsed.key_password))
}

/// Wie JS `encodeURIComponent`.
pub(crate) fn encode_uri_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Test-Gegenstück: so verschlüsselt die Login-Seite das `keyPassword`.
#[cfg(test)]
pub(crate) fn encrypt_payload(key: &[u8; 32], json: &str) -> String {
    let cipher = Aes256Gcm::new_from_slice(key).unwrap();
    let nonce = [7u8; NONCE_LEN];
    let sealed = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: json.as_bytes(),
                aad: PAYLOAD_AAD,
            },
        )
        .unwrap();
    STANDARD.encode([nonce.as_slice(), &sealed].concat())
}

/// Test-Gegenstück: Schlüssel aus dem URL-Fragment lesen (wie die Login-Seite).
#[cfg(test)]
pub(crate) fn key_from_url(url: &str) -> [u8; 32] {
    let fragment = url.split("#payload=").nth(1).unwrap().replace("%3A", ":");
    let key_b64 = fragment.split(':').nth(2).unwrap();
    let key_b64 = key_b64
        .replace("%2B", "+")
        .replace("%2F", "/")
        .replace("%3D", "=");
    STANDARD.decode(key_b64).unwrap().try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_carries_user_code_key_and_client_id_in_fragment_only() {
        let key = ForkKey::from_bytes([0xfb; 32]);
        let url = login_url("AB12CD", &key);
        let url = url.expose();
        let (base, fragment) = url.split_once('#').unwrap();
        assert_eq!(
            base,
            "https://account.proton.me/desktop/login?app=drive&pv=3"
        );
        assert!(fragment.starts_with("payload=0%3AAB12CD%3A"), "{fragment}");
        assert!(fragment.ends_with("%3Aexternal-drive"), "{fragment}");
        assert!(
            !fragment.contains('+') && !fragment.contains('/'),
            "{fragment}"
        );
        assert_eq!(key_from_url(url), [0xfb; 32]);
    }

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(
            encode_uri_component("0:AB:a+b/c=:x-y_z.~"),
            "0%3AAB%3Aa%2Bb%2Fc%3D%3Ax-y_z.~"
        );
    }

    #[test]
    fn decrypts_payload_and_rejects_tampering() {
        let raw = [3u8; 32];
        let key = ForkKey::from_bytes(raw);
        let payload = encrypt_payload(&raw, r#"{"type":"default","keyPassword":"kp-1"}"#);
        assert_eq!(decrypt_payload(&key, &payload).unwrap().expose(), "kp-1");

        let other = ForkKey::from_bytes([4u8; 32]);
        assert!(decrypt_payload(&other, &payload).is_err());
        let mut bytes = STANDARD.decode(&payload).unwrap();
        bytes[20] ^= 1;
        assert!(decrypt_payload(&key, &STANDARD.encode(bytes)).is_err());
        assert!(decrypt_payload(&key, "AAAA").is_err());
        assert!(decrypt_payload(&key, &encrypt_payload(&raw, r#"{"type":"x"}"#)).is_err());
    }
}
