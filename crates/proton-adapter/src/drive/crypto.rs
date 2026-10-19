//! Drive-Krypto über `proton-crypto` (rpgp-Backend, keine Eigenbau-Krypto; ADR-013).
//!
//! Schlüsselkette: `keyPassword` → User-Keys → Adress-Keys → Share-Key → Node-Keys (Ordner/Datei)
//! → Namen, `NodeHashKey`, Content-Session-Key → Blöcke.
//!
//! Neue Inhalte im „pre-crypto-refresh"-Format, das die offiziellen Apps sicher lesen: v4-Keys,
//! PKESK v3, SEIPD v1, keine AEAD – das Default-Profil des Rust-Backends (Test unten).
//!
//! Lesen ist **signaturgeprüft** (fail closed): Share- und Node-Passphrasen, Namen und das
//! Block-Manifest müssen von einem Adress-Key des Nutzers signiert sein, `NodeHashKey` und
//! `ContentKeyPacket` vom Node-Key. Sonst könnte der Server eigene Schlüssel unterschieben (die
//! Verschlüsselung an einen Schlüssel braucht nur dessen öffentlichen Teil) und mitlesen, was
//! danach hochgeladen wird.

use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use proton_crypto::crypto::{
    DataEncoding, Decryptor, DecryptorSync, Encryptor, EncryptorSync, KeyGenerator,
    KeyGeneratorSync, PGPProvider, PGPProviderSync, SessionKeyAlgorithm, Signer, SignerSync,
    VerifiedData, Verifier, VerifierSync,
};
use proton_crypto_account::{
    keys::{AddressKeys, LockedKey, UnlockedAddressKey, UnlockedUserKey, UserKeys},
    salts::KeySecret,
};
use sha2::{Digest, Sha256};
use todo_core::{CoreError, CoreResult};

use crate::secret::Secret;

pub(crate) type PrivateKey<P> = <P as PGPProvider>::PrivateKey;
pub(crate) type SessionKey<P> = <P as PGPProvider>::SessionKey;
pub(crate) type PublicKey<P> = <P as PGPProvider>::PublicKey;

/// Krypto-Fehler sind endgültig: Die Daten im Backend passen nicht zu unseren Schlüsseln.
fn crypto<E>(what: &str) -> impl FnOnce(E) -> CoreError + '_ {
    move |_| CoreError::RemoteRejected(format!("drive crypto: {what}"))
}

/// Ver-/Entschlüsselungsfelder eines neuen Knotens (Ordner oder Datei).
pub(crate) struct NewNode<P: PGPProvider> {
    pub(crate) key: PrivateKey<P>,
    pub(crate) name: String,
    pub(crate) hash: String,
    pub(crate) node_key: String,
    pub(crate) node_passphrase: String,
    pub(crate) node_passphrase_signature: String,
}

/// Content-Schlüssel einer neuen Datei.
pub(crate) struct ContentKey<P: PGPProvider> {
    pub(crate) session_key: SessionKey<P>,
    pub(crate) key_packet_b64: String,
    pub(crate) key_packet_signature: String,
}

/// Ein verschlüsselter Block samt Metadaten für `POST /drive/blocks`.
pub(crate) struct EncryptedBlock {
    pub(crate) data: Vec<u8>,
    /// SHA-256 der verschlüsselten Daten.
    pub(crate) hash: Vec<u8>,
    pub(crate) enc_signature: String,
}

/// Dünne Hülle um den PGP-Provider mit den Drive-Operationen.
#[derive(Clone)]
pub(crate) struct DriveCrypto<P> {
    p: P,
}

impl<P: PGPProviderSync> DriveCrypto<P> {
    pub(crate) fn new(provider: P) -> Self {
        Self { p: provider }
    }

    pub(crate) fn unlock_user_keys(
        &self,
        locked: Vec<LockedKey>,
        key_password: &Secret,
    ) -> CoreResult<Vec<UnlockedUserKey<P>>> {
        let secret = KeySecret::new(key_password.expose().as_bytes().to_vec());
        let keys = UserKeys::new(locked).unlock(&self.p, &secret).unlocked_keys;
        if keys.is_empty() {
            return Err(CoreError::RemoteRejected(
                "drive crypto: no user key could be unlocked".into(),
            ));
        }
        Ok(keys)
    }

    pub(crate) fn unlock_address_keys(
        &self,
        locked: Vec<LockedKey>,
        user_keys: &[UnlockedUserKey<P>],
        key_password: &Secret,
    ) -> CoreResult<Vec<UnlockedAddressKey<P>>> {
        let secret = KeySecret::new(key_password.expose().as_bytes().to_vec());
        let keys = AddressKeys::new(locked)
            .unlock(&self.p, user_keys, Some(&secret))
            .unlocked_keys;
        if keys.is_empty() {
            return Err(CoreError::RemoteRejected(
                "drive crypto: no address key could be unlocked".into(),
            ));
        }
        Ok(keys)
    }

    /// Entschlüsselt eine armierte Nachricht mit einem der `keys` (ohne Signaturprüfung; nur
    /// für Daten, deren Echtheit anders gesichert ist, z. B. eine signierte Passphrase).
    pub(crate) fn decrypt(&self, keys: &[&PrivateKey<P>], armored: &str) -> CoreResult<Vec<u8>> {
        let data = self
            .p
            .new_decryptor()
            .with_decryption_keys(keys.iter().copied())
            .decrypt(armored, DataEncoding::Armor)
            .map_err(crypto("decrypt"))?;
        Ok(data.as_bytes().to_vec())
    }

    /// Entschlüsselt und verlangt eine gültige eingebettete Signatur von einem der `verifiers`.
    fn decrypt_verified(
        &self,
        keys: &[&PrivateKey<P>],
        verifiers: &[PublicKey<P>],
        armored: &str,
        what: &'static str,
    ) -> CoreResult<Vec<u8>> {
        let data = self
            .p
            .new_decryptor()
            .with_decryption_keys(keys.iter().copied())
            .with_verification_keys(verifiers)
            .decrypt(armored, DataEncoding::Armor)
            .map_err(crypto(what))?;
        data.verification_result()
            .map_err(|_| CoreError::RemoteRejected(format!("drive crypto: {what} signature")))?;
        Ok(data.as_bytes().to_vec())
    }

    /// Prüft eine abgetrennte armierte Signatur über `data`.
    pub(crate) fn verify_detached(
        &self,
        verifiers: &[PublicKey<P>],
        data: &[u8],
        signature: &str,
        what: &'static str,
    ) -> CoreResult<()> {
        if verifiers.is_empty() {
            return Err(CoreError::RemoteRejected(format!(
                "drive crypto: {what}: no verification key"
            )));
        }
        self.p
            .new_verifier()
            .with_verification_keys(verifiers)
            .verify_detached(data, signature.as_bytes(), DataEncoding::Armor)
            .map(drop)
            .map_err(|_| CoreError::RemoteRejected(format!("drive crypto: {what} signature")))
    }

    /// Öffentliche Schlüssel (zum Prüfen von Signaturen).
    pub(crate) fn public_keys<'a>(
        &self,
        keys: impl IntoIterator<Item = &'a PrivateKey<P>>,
    ) -> CoreResult<Vec<PublicKey<P>>>
    where
        P: 'a,
    {
        keys.into_iter()
            .map(|k| {
                self.p
                    .private_key_to_public_key(k)
                    .map_err(crypto("public key"))
            })
            .collect()
    }

    /// Passphrase-geschützten armierten Schlüssel importieren.
    pub(crate) fn import_key(&self, armored: &str, passphrase: &[u8]) -> CoreResult<PrivateKey<P>> {
        self.p
            .private_key_import(armored, passphrase, DataEncoding::Armor)
            .map_err(crypto("import key"))
    }

    /// Share-Key: Passphrase ist an einen Adress-Key verschlüsselt und von ihm signiert.
    pub(crate) fn unlock_share(
        &self,
        address_keys: &[UnlockedAddressKey<P>],
        verifiers: &[PublicKey<P>],
        key: &str,
        passphrase: &str,
        signature: Option<&str>,
    ) -> CoreResult<PrivateKey<P>> {
        let keys: Vec<&PrivateKey<P>> = address_keys.iter().map(|k| &k.private_key).collect();
        let pass = self.decrypt(&keys, passphrase)?;
        let signature = signature.ok_or_else(|| {
            CoreError::RemoteRejected("drive crypto: share passphrase is not signed".into())
        })?;
        self.verify_detached(verifiers, &pass, signature, "share passphrase")?;
        self.import_key(key, &pass)
    }

    /// Node-Key: Passphrase ist an den Schlüssel des Elternknotens verschlüsselt und von einem
    /// Adress-Key signiert (`NodePassphraseSignature`).
    pub(crate) fn unlock_node(
        &self,
        parent: &PrivateKey<P>,
        verifiers: &[PublicKey<P>],
        node_key: &str,
        node_passphrase: &str,
        signature: Option<&str>,
    ) -> CoreResult<PrivateKey<P>> {
        let pass = self.decrypt(&[parent], node_passphrase)?;
        let signature = signature.ok_or_else(|| {
            CoreError::RemoteRejected("drive crypto: node passphrase is not signed".into())
        })?;
        self.verify_detached(verifiers, &pass, signature, "node passphrase")?;
        self.import_key(node_key, &pass)
    }

    /// Name eines Knotens: an den Eltern-Key verschlüsselt, von einem Adress-Key signiert.
    pub(crate) fn decrypt_name(
        &self,
        parent: &PrivateKey<P>,
        verifiers: &[PublicKey<P>],
        armored: &str,
    ) -> CoreResult<String> {
        let bytes = self.decrypt_verified(&[parent], verifiers, armored, "name")?;
        String::from_utf8(bytes).map_err(|_| CoreError::RemoteRejected("drive crypto: name".into()))
    }

    /// `NodeHashKey` eines Ordners (an den eigenen Node-Key verschlüsselt und von ihm – bei
    /// älteren Clients vom Adress-Key – signiert).
    pub(crate) fn decrypt_hash_key(
        &self,
        node: &PrivateKey<P>,
        verifiers: &[PublicKey<P>],
        armored: &str,
    ) -> CoreResult<Vec<u8>> {
        let mut allowed = self.public_keys([node])?;
        allowed.extend(verifiers.iter().cloned());
        self.decrypt_verified(&[node], &allowed, armored, "hash key")
    }

    pub(crate) fn encrypt(
        &self,
        recipient: &PrivateKey<P>,
        data: &[u8],
        signer: Option<&PrivateKey<P>>,
    ) -> CoreResult<String> {
        let public = self
            .p
            .private_key_to_public_key(recipient)
            .map_err(crypto("public key"))?;
        let mut encryptor = self.p.new_encryptor().with_encryption_key(&public);
        if let Some(signer) = signer {
            encryptor = encryptor.with_signing_key(signer);
        }
        let out = encryptor
            .encrypt_raw(data, DataEncoding::Armor)
            .map_err(crypto("encrypt"))?;
        String::from_utf8(out).map_err(|_| CoreError::Internal("armor is not utf-8".into()))
    }

    pub(crate) fn sign(&self, key: &PrivateKey<P>, data: &[u8]) -> CoreResult<String> {
        let sig = self
            .p
            .new_signer()
            .with_signing_key(key)
            .sign_detached(data, DataEncoding::Armor)
            .map_err(crypto("sign"))?;
        String::from_utf8(sig).map_err(|_| CoreError::Internal("armor is not utf-8".into()))
    }

    /// Neuer Knoten unter `parent`: eigener Schlüssel mit zufälliger Passphrase, Name
    /// verschlüsselt an den Eltern-Key und mit dem Adress-Key signiert.
    pub(crate) fn new_node(
        &self,
        parent: &PrivateKey<P>,
        parent_hash_key: &[u8],
        address_key: &PrivateKey<P>,
        name: &str,
    ) -> CoreResult<NewNode<P>> {
        let passphrase = STANDARD.encode(random::<32>()?);
        let key = self
            .p
            .new_key_generator()
            .with_user_id("Drive key", "noreply@protonmail.com")
            .generate()
            .map_err(crypto("generate key"))?;
        let node_key = {
            let locked = self
                .p
                .private_key_export(&key, passphrase.as_bytes(), DataEncoding::Armor)
                .map_err(crypto("export key"))?;
            String::from_utf8(locked.as_ref().to_vec())
                .map_err(|_| CoreError::Internal("armor is not utf-8".into()))?
        };
        Ok(NewNode {
            name: self.encrypt(parent, name.as_bytes(), Some(address_key))?,
            hash: name_hash(parent_hash_key, name),
            node_key,
            node_passphrase: self.encrypt(parent, passphrase.as_bytes(), None)?,
            node_passphrase_signature: self.sign(address_key, passphrase.as_bytes())?,
            key,
        })
    }

    /// Hash-Key eines neuen Ordners: wie bei den offiziellen Clients der Base64-Text von
    /// 32 Zufallsbytes, an den eigenen Node-Key verschlüsselt und signiert.
    /// Liefert (Schlüssel, armierte Nachricht).
    pub(crate) fn new_folder_hash_key(
        &self,
        node: &PrivateKey<P>,
    ) -> CoreResult<(Vec<u8>, String)> {
        let text = STANDARD.encode(random::<32>()?);
        let armored = self.encrypt(node, text.as_bytes(), Some(node))?;
        Ok((text.into_bytes(), armored))
    }

    /// Content-Key einer neuen Datei: Session-Key, an den Node-Key verschlüsselt
    /// (`ContentKeyPacket`), Klartext-Key mit dem Node-Key signiert.
    pub(crate) fn new_content_key(&self, node: &PrivateKey<P>) -> CoreResult<ContentKey<P>> {
        let session_key = self
            .p
            .session_key_generate(SessionKeyAlgorithm::Aes256)
            .map_err(crypto("session key"))?;
        let public = self
            .p
            .private_key_to_public_key(node)
            .map_err(crypto("public key"))?;
        let packet = self
            .p
            .new_encryptor()
            .with_encryption_key(&public)
            .encrypt_session_key(&session_key)
            .map_err(crypto("key packet"))?;
        let key_packet_signature = {
            let (raw, _) = self
                .p
                .session_key_export(&session_key)
                .map_err(crypto("session key export"))?;
            self.sign(node, raw.as_ref())?
        };
        Ok(ContentKey {
            key_packet_signature,
            key_packet_b64: STANDARD.encode(packet),
            session_key,
        })
    }

    /// Session-Key aus dem `ContentKeyPacket` (base64) einer bestehenden Datei; die Signatur
    /// des Node-Keys über den Klartext-Key (`ContentKeyPacketSignature`) muss passen.
    pub(crate) fn session_key(
        &self,
        node: &PrivateKey<P>,
        packet_b64: &str,
        signature: Option<&str>,
    ) -> CoreResult<SessionKey<P>> {
        let packet = STANDARD
            .decode(packet_b64)
            .map_err(|_| CoreError::RemoteRejected("drive crypto: key packet base64".into()))?;
        let session_key = self
            .p
            .new_decryptor()
            .with_decryption_key(node)
            .decrypt_session_key(&packet)
            .map_err(crypto("session key"))?;
        let signature = signature.ok_or_else(|| {
            CoreError::RemoteRejected("drive crypto: content key is not signed".into())
        })?;
        {
            let (raw, _) = self
                .p
                .session_key_export(&session_key)
                .map_err(crypto("session key export"))?;
            self.verify_detached(
                &self.public_keys([node])?,
                raw.as_ref(),
                signature,
                "content key",
            )?;
        }
        Ok(session_key)
    }

    /// Verschlüsselt einen Block (nur Datenpaket); die Klartext-Signatur des Adress-Keys wird an
    /// den Node-Key verschlüsselt (`EncSignature`).
    pub(crate) fn encrypt_block(
        &self,
        session_key: &SessionKey<P>,
        node: &PrivateKey<P>,
        address_key: &PrivateKey<P>,
        plain: &[u8],
    ) -> CoreResult<EncryptedBlock> {
        let data = self
            .p
            .new_encryptor()
            .with_session_key_ref(session_key)
            .encrypt_raw(plain, DataEncoding::Bytes)
            .map_err(crypto("encrypt block"))?;
        let signature = self
            .p
            .new_signer()
            .with_signing_key(address_key)
            .sign_detached(plain, DataEncoding::Bytes)
            .map_err(crypto("sign block"))?;
        Ok(EncryptedBlock {
            enc_signature: self.encrypt(node, &signature, None)?,
            hash: Sha256::digest(&data).to_vec(),
            data,
        })
    }

    pub(crate) fn decrypt_block(
        &self,
        session_key: &SessionKey<P>,
        data: &[u8],
    ) -> CoreResult<Vec<u8>> {
        let plain = self
            .p
            .new_decryptor()
            .with_session_key_ref(session_key)
            .decrypt(data, DataEncoding::Bytes)
            .map_err(crypto("decrypt block"))?;
        Ok(plain.as_bytes().to_vec())
    }
}

/// Namens-Hash für die Duplikat-Erkennung: hex(HMAC-SHA256(NodeHashKey des Elternordners, Name)).
pub(crate) fn name_hash(parent_hash_key: &[u8], name: &str) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(parent_hash_key)
        .expect("HMAC accepts any key length");
    mac.update(name.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Block-Verifier: `code XOR erste Bytes des verschlüsselten Blocks` (fehlende als 0).
pub(crate) fn verification_token(code: &[u8], encrypted_block: &[u8]) -> Vec<u8> {
    code.iter()
        .enumerate()
        .map(|(i, c)| c ^ encrypted_block.get(i).copied().unwrap_or(0))
        .collect()
}

pub(crate) fn random<const N: usize>() -> CoreResult<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|e| CoreError::Internal(format!("random: {e}")))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use proton_crypto::crypto::AccessKeyInfo;

    use super::*;

    fn provider() -> DriveCrypto<impl PGPProviderSync> {
        DriveCrypto::new(proton_crypto::new_pgp_provider())
    }

    /// Tag und Versionsbyte des ersten OpenPGP-Pakets (neues oder altes Header-Format).
    fn packet_version(bytes: &[u8], expected_tag: u8) -> u8 {
        let header = bytes[0];
        assert!(header & 0x80 != 0, "no OpenPGP packet");
        let (tag, body) = if header & 0x40 != 0 {
            let offset = match bytes[1] {
                0..=191 => 2,
                192..=223 => 3,
                255 => 6,
                _ => 2,
            };
            (header & 0x3f, offset)
        } else {
            let offset = match header & 0x03 {
                0 => 2,
                1 => 3,
                2 => 5,
                _ => 1,
            };
            ((header >> 2) & 0x0f, offset)
        };
        assert_eq!(tag, expected_tag, "packet tag");
        bytes[body]
    }

    #[test]
    fn name_hash_is_hex_hmac_sha256() {
        // RFC 4231, Testfall 2.
        assert_eq!(
            name_hash(b"Jefe", "what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn verification_token_xors_prefix_and_pads() {
        assert_eq!(
            verification_token(&[0xff, 0x0f, 0x00], &[0x01, 0x02, 0x03, 0x04]),
            [0xfe, 0x0d, 0x03]
        );
        assert_eq!(verification_token(&[0xaa, 0xbb], &[0x01]), [0xab, 0xbb]);
    }

    #[test]
    fn node_file_and_block_roundtrip_in_legacy_format() {
        let c = provider();
        let generate = || {
            c.p.new_key_generator()
                .with_user_id("t", "t@example.org")
                .generate()
                .unwrap()
        };
        let address = generate();
        let verifiers = c.public_keys([&address]).unwrap();
        let parent = generate();

        let folder = c.new_node(&parent, &[9; 32], &address, "Tasks").unwrap();
        let folder_key = c
            .unlock_node(
                &parent,
                &verifiers,
                &folder.node_key,
                &folder.node_passphrase,
                Some(&folder.node_passphrase_signature),
            )
            .unwrap();
        assert_eq!(
            c.decrypt_name(&parent, &verifiers, &folder.name).unwrap(),
            "Tasks"
        );
        assert_eq!(folder.hash, name_hash(&[9; 32], "Tasks"));
        let (plain_hash_key, armored) = c.new_folder_hash_key(&folder_key).unwrap();
        let hash_key = c
            .decrypt_hash_key(&folder_key, &verifiers, &armored)
            .unwrap();
        assert_eq!(hash_key, plain_hash_key);
        assert_eq!(
            hash_key.len(),
            44,
            "Base64-Text wie bei den offiziellen Clients"
        );

        let file = c
            .new_node(&folder_key, &hash_key, &address, "1-3.seg")
            .unwrap();
        let file_key = c
            .unlock_node(
                &folder_key,
                &verifiers,
                &file.node_key,
                &file.node_passphrase,
                Some(&file.node_passphrase_signature),
            )
            .unwrap();
        let content = c.new_content_key(&file.key).unwrap();
        let session_key = c
            .session_key(
                &file_key,
                &content.key_packet_b64,
                Some(&content.key_packet_signature),
            )
            .unwrap();
        let plain = b"segment".repeat(100);
        let block = c
            .encrypt_block(&content.session_key, &file.key, &address, &plain)
            .unwrap();
        assert_eq!(c.decrypt_block(&session_key, &block.data).unwrap(), plain);
        assert_eq!(block.hash, Sha256::digest(&block.data).to_vec());

        // Pre-crypto-refresh: v4-Keys, PKESK v3, SEIPD v1.
        assert_eq!(file.key.version(), 4);
        let packet = STANDARD.decode(&content.key_packet_b64).unwrap();
        assert_eq!(packet_version(&packet, 1), 3, "PKESK v3");
        assert_eq!(packet_version(&block.data, 18), 1, "SEIPD v1");
    }

    /// Was der Server ohne die privaten Schlüssel des Nutzers erzeugen kann (Verschlüsselung an
    /// öffentliche Schlüssel, eigene Signaturen), wird abgelehnt.
    #[test]
    fn unsigned_or_foreign_signed_material_is_rejected() {
        let c = provider();
        let generate = || {
            c.p.new_key_generator()
                .with_user_id("t", "t@example.org")
                .generate()
                .unwrap()
        };
        let address = generate();
        let verifiers = c.public_keys([&address]).unwrap();
        let server = generate();
        let parent = generate();

        // Vom Server erzeugter Knoten: Passphrase nicht oder mit fremdem Key signiert.
        let forged = c.new_node(&parent, &[1; 32], &server, "Tasks").unwrap();
        for signature in [None, Some(forged.node_passphrase_signature.as_str())] {
            assert!(c
                .unlock_node(
                    &parent,
                    &verifiers,
                    &forged.node_key,
                    &forged.node_passphrase,
                    signature
                )
                .is_err());
        }
        // Name mit fremder Signatur bzw. ganz ohne.
        assert!(c.decrypt_name(&parent, &verifiers, &forged.name).is_err());
        let unsigned = c.encrypt(&parent, b"Tasks", None).unwrap();
        assert!(c.decrypt_name(&parent, &verifiers, &unsigned).is_err());
        // Hash-Key, den nur der Server signiert hat.
        let (_, foreign_hash_key) = c.new_folder_hash_key(&server).unwrap();
        let node = generate();
        let reencrypted = c.encrypt(&node, b"hash-key", Some(&server)).unwrap();
        assert!(c.decrypt_hash_key(&node, &verifiers, &reencrypted).is_err());
        let _ = foreign_hash_key;
        // Content-Key mit Signatur eines anderen Schlüssels.
        let file = generate();
        let content = c.new_content_key(&file).unwrap();
        let other = c.new_content_key(&server).unwrap();
        assert!(c
            .session_key(
                &file,
                &content.key_packet_b64,
                Some(&other.key_packet_signature)
            )
            .is_err());
        assert!(c.session_key(&file, &content.key_packet_b64, None).is_err());
    }
}
