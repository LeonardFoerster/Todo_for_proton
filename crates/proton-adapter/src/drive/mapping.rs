//! Proton-Drive-Typen (JSON der v1-Share-Endpunkte) und ihre Abbildung auf Port-Typen.
//! Nichts hiervon verlässt das Crate.

use proton_crypto_account::keys::LockedKey;
use serde::Deserialize;
use todo_core::{
    domain::Timestamp,
    ports::{RemoteEntry, RemotePath, Revision},
};

pub(crate) const LINK_FOLDER: u8 = 1;
pub(crate) const LINK_FILE: u8 = 2;
/// `State` eines Links: 0 = Entwurf, 1 = aktiv, 2 = im Papierkorb.
pub(crate) const STATE_DRAFT: u8 = 0;
pub(crate) const STATE_ACTIVE: u8 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Link {
    #[serde(rename = "LinkID")]
    pub(crate) link_id: String,
    #[serde(rename = "ParentLinkID", default)]
    pub(crate) parent_link_id: Option<String>,
    #[serde(rename = "Type")]
    pub(crate) kind: u8,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) state: Option<u8>,
    /// Verschlüsselte Größe (v1).
    #[serde(default)]
    pub(crate) size: Option<u64>,
    /// Unix-Sekunden.
    #[serde(default)]
    pub(crate) modify_time: Option<i64>,
    #[serde(default)]
    pub(crate) trashed: Option<i64>,
    pub(crate) node_key: String,
    pub(crate) node_passphrase: String,
    /// Signatur eines Adress-Keys über die Klartext-Passphrase (Pflicht beim Entsperren).
    #[serde(default)]
    pub(crate) node_passphrase_signature: Option<String>,
    #[serde(default)]
    pub(crate) file_properties: Option<FileProperties>,
    #[serde(default)]
    pub(crate) folder_properties: Option<FolderProperties>,
}

impl Link {
    pub(crate) fn is_folder(&self) -> bool {
        self.kind == LINK_FOLDER
    }

    /// Aktive Datei mit committeter Revision (keine Entwürfe, nichts im Papierkorb).
    pub(crate) fn active_revision(&self) -> Option<&str> {
        if self.kind != LINK_FILE || !self.is_active() {
            return None;
        }
        self.file_properties
            .as_ref()?
            .active_revision
            .as_ref()
            .map(|r| r.id.as_str())
    }

    pub(crate) fn is_active(&self) -> bool {
        self.state.unwrap_or(STATE_ACTIVE) == STATE_ACTIVE && self.trashed.is_none()
    }

    pub(crate) fn is_draft(&self) -> bool {
        self.state == Some(STATE_DRAFT)
    }

    pub(crate) fn content_key_packet(&self) -> Option<&str> {
        self.file_properties.as_ref()?.content_key_packet.as_deref()
    }

    pub(crate) fn content_key_packet_signature(&self) -> Option<&str> {
        self.file_properties
            .as_ref()?
            .content_key_packet_signature
            .as_deref()
    }

    pub(crate) fn node_hash_key(&self) -> Option<&str> {
        self.folder_properties.as_ref()?.node_hash_key.as_deref()
    }

    /// Port-Sicht auf eine aktive Datei. `size` ist hier die verschlüsselte Speichergröße
    /// (die Klartextgröße stünde nur im verschlüsselten XAttr).
    pub(crate) fn entry(&self, path: RemotePath) -> Option<RemoteEntry> {
        let revision = self.active_revision()?;
        Some(RemoteEntry {
            path,
            size: self.size.unwrap_or(0),
            revision: Revision(revision.to_owned()),
            modified: Timestamp::from_millis(self.modify_time.unwrap_or(0).saturating_mul(1000)),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct FileProperties {
    #[serde(default)]
    pub(crate) content_key_packet: Option<String>,
    /// Signatur des Node-Keys über den Klartext-Session-Key.
    #[serde(default)]
    pub(crate) content_key_packet_signature: Option<String>,
    #[serde(default)]
    pub(crate) active_revision: Option<ActiveRevision>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ActiveRevision {
    #[serde(rename = "ID")]
    pub(crate) id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct FolderProperties {
    #[serde(default)]
    pub(crate) node_hash_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct LinkResponse {
    pub(crate) link: Link,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ChildrenResponse {
    #[serde(default)]
    pub(crate) links: Vec<Link>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct UsersResponse {
    pub(crate) user: UserKeys,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct UserKeys {
    pub(crate) keys: Vec<LockedKey>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct AddressesResponse {
    pub(crate) addresses: Vec<Address>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Address {
    #[serde(rename = "ID")]
    pub(crate) id: String,
    pub(crate) email: String,
    pub(crate) keys: Vec<LockedKey>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct VolumesResponse {
    pub(crate) volumes: Vec<Volume>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Volume {
    #[serde(rename = "VolumeID")]
    pub(crate) volume_id: String,
    #[serde(default)]
    pub(crate) state: Option<u8>,
    pub(crate) share: VolumeShare,
}

#[derive(Deserialize)]
pub(crate) struct VolumeShare {
    #[serde(rename = "ShareID")]
    pub(crate) share_id: String,
    #[serde(rename = "LinkID")]
    pub(crate) link_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Share {
    #[serde(rename = "AddressID")]
    pub(crate) address_id: String,
    #[serde(rename = "VolumeID", default)]
    pub(crate) volume_id: Option<String>,
    #[serde(rename = "LinkID", default)]
    pub(crate) link_id: Option<String>,
    pub(crate) key: String,
    pub(crate) passphrase: String,
    /// Signatur des Adress-Keys über die Klartext-Passphrase.
    #[serde(default)]
    pub(crate) passphrase_signature: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct CreatedFolder {
    pub(crate) folder: CreatedId,
}

#[derive(Deserialize)]
pub(crate) struct CreatedId {
    #[serde(rename = "ID")]
    pub(crate) id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct CreatedFile {
    pub(crate) file: CreatedFileIds,
}

#[derive(Deserialize)]
pub(crate) struct CreatedFileIds {
    #[serde(rename = "ID")]
    pub(crate) id: String,
    #[serde(rename = "RevisionID")]
    pub(crate) revision_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct CreatedRevision {
    pub(crate) revision: CreatedId,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Verification {
    pub(crate) verification_code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct UploadLinks {
    pub(crate) upload_links: Vec<UploadLink>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct UploadLink {
    #[serde(rename = "BareURL")]
    pub(crate) bare_url: String,
    pub(crate) token: String,
    #[serde(default)]
    pub(crate) index: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct RevisionResponse {
    pub(crate) revision: RevisionBlocks,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct RevisionBlocks {
    #[serde(default)]
    pub(crate) blocks: Vec<Block>,
    /// Signatur eines Adress-Keys über die aneinandergehängten Block-Hashes.
    #[serde(default)]
    pub(crate) manifest_signature: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Block {
    pub(crate) index: u32,
    #[serde(rename = "BareURL")]
    pub(crate) bare_url: String,
    pub(crate) token: String,
    /// SHA-256 des verschlüsselten Blocks (base64).
    #[serde(default)]
    pub(crate) hash: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct LatestEvent {
    #[serde(rename = "EventID")]
    pub(crate) event_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Events {
    #[serde(rename = "EventID")]
    pub(crate) event_id: String,
    #[serde(default)]
    pub(crate) events: Vec<Event>,
    #[serde(default)]
    pub(crate) more: u8,
    #[serde(default)]
    pub(crate) refresh: u8,
}

/// `EventType`: 0 = gelöscht (nur `LinkID`), 1 = angelegt, 2 = geändert (auch Papierkorb),
/// 3 = Metadaten geändert.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Event {
    pub(crate) event_type: u8,
    pub(crate) link: serde_json::Value,
}

pub(crate) const EVENT_DELETE: u8 = 0;

/// Fehlerdetails eines Namenskonflikts (Code 2500).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ConflictDetails {
    #[serde(rename = "ConflictLinkID", default)]
    pub(crate) link_id: Option<String>,
    #[serde(rename = "ConflictRevisionID", default)]
    pub(crate) revision_id: Option<String>,
    #[serde(rename = "ConflictDraftRevisionID", default)]
    pub(crate) draft_revision_id: Option<String>,
}

/// Einzelantworten von `…_multiple`-Endpunkten.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct MultiResponse {
    #[serde(default)]
    pub(crate) responses: Vec<LinkResponseCode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct LinkResponseCode {
    pub(crate) response: CodeOnly,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct CodeOnly {
    pub(crate) code: i64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn link(state: u8, trashed: Option<i64>) -> Link {
        serde_json::from_value(json!({
            "LinkID": "l1", "ParentLinkID": "p1", "Type": 2, "Name": "n", "State": state,
            "Size": 1077, "ModifyTime": 1_760_000_000, "Trashed": trashed,
            "NodeKey": "k", "NodePassphrase": "p",
            "FileProperties": { "ContentKeyPacket": "c", "ActiveRevision": { "ID": "r1" } },
            "FolderProperties": null,
        }))
        .unwrap()
    }

    #[test]
    fn active_file_maps_to_entry() {
        let path = RemotePath::new("Tasks/a.json").unwrap();
        let entry = link(1, None).entry(path.clone()).unwrap();
        assert_eq!(entry.path, path);
        assert_eq!(entry.revision, Revision("r1".into()));
        assert_eq!(entry.size, 1077);
        assert_eq!(entry.modified, Timestamp::from_millis(1_760_000_000_000));
    }

    #[test]
    fn drafts_and_trashed_files_are_not_entries() {
        let path = RemotePath::new("a").unwrap();
        assert!(link(0, None).entry(path.clone()).is_none());
        assert!(link(2, None).entry(path.clone()).is_none());
        assert!(link(1, Some(5)).entry(path).is_none());
        assert!(link(0, None).is_draft());
    }
}
