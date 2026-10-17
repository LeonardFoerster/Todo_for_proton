//! `ProtonDriveRemoteStore`: der App-Ordner im Haupt-Share („Meine Dateien") von Proton Drive.
//!
//! Endpunkte: v1-Share-Endpunkte (live erprobt in P2-05) plus v2 für Block-Verifikation, Commit
//! und endgültiges Löschen aus dem Papierkorb. Pfade werden über entschlüsselte Namen aufgelöst;
//! Ordner (Schlüssel) und Ordnerinhalte liegen in einem Speicher-Cache, der bei jedem Schreiben
//! und jedem „nicht gefunden" des betroffenen Ordners verworfen wird.
//!
//! Änderungserkennung über Volume-Events (Proton-Regel: kein Polling per Listing).

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::UnlockedAddressKey;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use todo_core::{
    domain::Timestamp,
    ports::{
        RemoteChanges, RemoteCursor, RemoteDir, RemoteEntry, RemoteListing, RemotePath,
        RemoteStore, Revision,
    },
    CoreError, CoreResult,
};

use super::{
    crypto::{random, verification_token, DriveCrypto, PrivateKey, SessionKey},
    mapping::{
        AddressesResponse, ChildrenResponse, ConflictDetails, CreatedFile, CreatedFolder,
        CreatedRevision, Events, LatestEvent, Link, LinkResponse, MultiResponse, RevisionResponse,
        Share, UploadLinks, UsersResponse, Verification, VolumesResponse, EVENT_DELETE,
        STATE_ACTIVE,
    },
};
use crate::{
    api_client::Request,
    auth::{session::Session, ProtonAuthProvider},
    error::{code, ApiError},
};

/// Blockgröße der offiziellen Clients.
const BLOCK_SIZE: usize = 4 * 1024 * 1024;
const CHILDREN_PAGE_SIZE: usize = 150;
const BLOCKS_PAGE_SIZE: usize = 50;
/// Höchstens so viele Event-Seiten je `changes_since` (Rest beim nächsten Aufruf).
const MAX_EVENT_PAGES: usize = 20;
/// Tiefe, bis zu der unbekannte Elternordner aus Events nachgeladen werden.
const MAX_RESOLVE_DEPTH: usize = 16;

/// PGP-Provider mit thread-sicheren Schlüsseltypen.
pub(crate) trait Pgp:
    PGPProviderSync<PrivateKey: Send + Sync, PublicKey: Send + Sync, SessionKey: Send + Sync>
    + Send
    + Sync
    + 'static
{
}

impl<T> Pgp for T where
    T: PGPProviderSync<PrivateKey: Send + Sync, PublicKey: Send + Sync, SessionKey: Send + Sync>
        + Send
        + Sync
        + 'static
{
}

/// [`RemoteStore`] auf Proton Drive. Teilt die Sitzung mit dem [`ProtonAuthProvider`].
pub struct ProtonDriveRemoteStore {
    inner: Arc<dyn RemoteStore>,
}

impl std::fmt::Debug for ProtonDriveRemoteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtonDriveRemoteStore")
            .finish_non_exhaustive()
    }
}

impl ProtonDriveRemoteStore {
    /// Store für das angemeldete Konto von `auth`. Schlüssel werden beim ersten Zugriff geladen;
    /// nach einem Kontowechsel einen neuen Store anlegen.
    ///
    /// # Errors
    /// `CoreError::Internal` ohne Zufallsquelle.
    pub fn new(auth: &ProtonAuthProvider) -> CoreResult<Self> {
        let drive = Drive::new(
            DriveCrypto::new(proton_crypto::new_pgp_provider()),
            auth.session(),
        )?;
        Ok(Self {
            inner: Arc::new(drive),
        })
    }
}

#[async_trait::async_trait]
impl RemoteStore for ProtonDriveRemoteStore {
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir> {
        self.inner.open_app_root(name).await
    }

    async fn list(
        &self,
        dir: &RemotePath,
        since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing> {
        self.inner.list(dir, since).await
    }

    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>> {
        self.inner.read(path).await
    }

    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry> {
        self.inner.create(path, bytes).await
    }

    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry> {
        self.inner.replace(path, bytes, expected).await
    }

    async fn delete(&self, path: &RemotePath) -> CoreResult<()> {
        self.inner.delete(path).await
    }

    async fn changes_since(
        &self,
        cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>> {
        self.inner.changes_since(cursor).await
    }
}

/// Entsperrter Zugang zum Haupt-Share.
struct Context<P: Pgp> {
    volume_id: String,
    share_id: String,
    address_email: String,
    address_id: String,
    address_keys: Vec<UnlockedAddressKey<P>>,
    root: Arc<Folder<P>>,
}

impl<P: Pgp> Context<P> {
    /// Signiert Namen, Passphrasen, Blöcke und Manifeste: primärer Adress-Key.
    fn signing_key(&self) -> &PrivateKey<P> {
        let key = self
            .address_keys
            .iter()
            .find(|k| k.primary)
            .unwrap_or(&self.address_keys[0]);
        &key.private_key
    }
}

/// Ein Ordner mit entsperrtem Schlüssel. `path` = Pfad unter „Meine Dateien" ("" = Wurzel).
struct Folder<P: Pgp> {
    link_id: String,
    path: String,
    key: PrivateKey<P>,
    hash_key: Vec<u8>,
}

/// Inhalt eines Ordners: Klarname → Link (aktive Einträge haben Vorrang).
type Children = Arc<HashMap<String, Link>>;

struct Cache<P: Pgp> {
    folders: HashMap<String, Arc<Folder<P>>>,
    folder_by_link: HashMap<String, String>,
    children: HashMap<String, Children>,
    /// Zuletzt gesehene Pfade von Dateien (für Lösch-Events, die nur die `LinkID` tragen).
    file_by_link: HashMap<String, RemotePath>,
    /// Geöffnete App-Ordner; Events außerhalb werden ignoriert.
    roots: HashSet<String>,
}

impl<P: Pgp> Default for Cache<P> {
    fn default() -> Self {
        Self {
            folders: HashMap::new(),
            folder_by_link: HashMap::new(),
            children: HashMap::new(),
            file_by_link: HashMap::new(),
            roots: HashSet::new(),
        }
    }
}

impl<P: Pgp> Cache<P> {
    fn add_folder(&mut self, folder: Arc<Folder<P>>) {
        self.folder_by_link
            .insert(folder.link_id.clone(), folder.path.clone());
        self.folders.insert(folder.path.clone(), folder);
    }

    fn folder_for_link(&self, link_id: &str) -> Option<Arc<Folder<P>>> {
        self.folders.get(self.folder_by_link.get(link_id)?).cloned()
    }

    fn in_roots(&self, path: &str) -> bool {
        self.roots
            .iter()
            .any(|root| path == root || path.starts_with(&format!("{root}/")))
    }
}

pub(crate) struct Drive<P: Pgp> {
    crypto: DriveCrypto<P>,
    session: Arc<Session>,
    context: tokio::sync::Mutex<Option<Arc<Context<P>>>>,
    cache: Mutex<Cache<P>>,
    /// Kennt der Server bei Entwürfen; erlaubt, eigene abgebrochene Uploads zu erkennen.
    client_uid: String,
    block_size: usize,
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

fn split(path: &RemotePath) -> (&str, &str) {
    path.as_str()
        .rsplit_once('/')
        .unwrap_or(("", path.as_str()))
}

fn decode<T: DeserializeOwned>(value: Value, what: &str) -> CoreResult<T> {
    serde_json::from_value(value)
        .map_err(|e| CoreError::RemoteRejected(format!("unexpected {what} response: {e}")))
}

fn now() -> Timestamp {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    Timestamp::from_millis(i64::try_from(ms).unwrap_or(i64::MAX))
}

fn mime_type(name: &str) -> &'static str {
    if name.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

/// Namens- oder Revisionskonflikt (Code 2500, auch als HTTP 409).
fn conflict_details(err: &ApiError) -> Option<ConflictDetails> {
    let is_conflict = err.code() == Some(code::ALREADY_EXISTS) || err.status() == Some(409);
    is_conflict.then(|| {
        err.details()
            .and_then(|d| serde_json::from_str(d).ok())
            .unwrap_or_default()
    })
}

/// Einzelantworten eines `…_multiple`-Aufrufs prüfen; 2501 (schon weg) ist kein Fehler.
fn check_multi(body: Value, what: &str) -> CoreResult<()> {
    let multi: MultiResponse = decode(body, what)?;
    match multi
        .responses
        .iter()
        .map(|r| r.response.code)
        .find(|c| *c != code::OK && *c != code::NOT_EXISTS)
    {
        Some(failed) => Err(CoreError::RemoteRejected(format!("{what}: code {failed}"))),
        None => Ok(()),
    }
}

impl<P: Pgp> Drive<P> {
    pub(crate) fn new(crypto: DriveCrypto<P>, session: Arc<Session>) -> CoreResult<Self> {
        Ok(Self {
            crypto,
            session,
            context: tokio::sync::Mutex::new(None),
            cache: Mutex::new(Cache::default()),
            client_uid: format!("tasks-{}", hex::encode(random::<12>()?)),
            block_size: BLOCK_SIZE,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_block_size(mut self, block_size: usize) -> Self {
        self.block_size = block_size;
        self
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Cache<P>> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn get<T: DeserializeOwned>(&self, route: &'static str, path: String) -> CoreResult<T> {
        self.session.send_as(Request::get(route, path)).await
    }

    async fn post(&self, route: &'static str, path: String, body: Value) -> CoreResult<Value> {
        self.session
            .send(Request::post(route, path).json(body))
            .await
    }

    /// Lädt Schlüssel, Volume, Share und Wurzelordner (einmal je Store).
    async fn context(&self) -> CoreResult<Arc<Context<P>>> {
        let mut slot = self.context.lock().await;
        if let Some(ctx) = slot.as_ref() {
            return Ok(Arc::clone(ctx));
        }
        let key_password = self.session.key_password().await?;
        let users: UsersResponse = self.get("/core/v4/users", "/core/v4/users".into()).await?;
        let user_keys = self
            .crypto
            .unlock_user_keys(users.user.keys, &key_password)?;

        let volumes: VolumesResponse = self.get("/drive/volumes", "/drive/volumes".into()).await?;
        let volume = volumes
            .volumes
            .into_iter()
            .find(|v| v.state.unwrap_or(STATE_ACTIVE) == STATE_ACTIVE)
            .ok_or_else(|| CoreError::RemoteRejected("no active drive volume".into()))?;
        let share_id = volume.share.share_id;
        let share: Share = self
            .get("/drive/shares/{share}", format!("/drive/shares/{share_id}"))
            .await?;
        let addresses: AddressesResponse = self
            .get("/core/v4/addresses", "/core/v4/addresses".into())
            .await?;
        let address = addresses
            .addresses
            .into_iter()
            .find(|a| a.id == share.address_id)
            .ok_or_else(|| CoreError::RemoteRejected("share address not found".into()))?;
        let address_keys =
            self.crypto
                .unlock_address_keys(address.keys, &user_keys, &key_password)?;
        let share_key = self
            .crypto
            .unlock_share(&address_keys, &share.key, &share.passphrase)?;

        let root_id = volume.share.link_id;
        let root: LinkResponse = self
            .get(
                "/drive/shares/{share}/links/{link}",
                format!("/drive/shares/{share_id}/links/{root_id}"),
            )
            .await?;
        let root = self.unlock_folder(&share_key, &root.link, String::new())?;
        self.cache().add_folder(Arc::clone(&root));

        let ctx = Arc::new(Context {
            volume_id: volume.volume_id,
            share_id,
            address_email: address.email,
            address_id: address.id,
            address_keys,
            root,
        });
        *slot = Some(Arc::clone(&ctx));
        Ok(ctx)
    }

    fn unlock_folder(
        &self,
        parent_key: &PrivateKey<P>,
        link: &Link,
        path: String,
    ) -> CoreResult<Arc<Folder<P>>> {
        let key = self
            .crypto
            .unlock_node(parent_key, &link.node_key, &link.node_passphrase)?;
        let hash_key = match link.node_hash_key() {
            Some(armored) => self.crypto.decrypt_hash_key(&key, armored)?,
            None => return Err(CoreError::RemoteRejected("folder without hash key".into())),
        };
        Ok(Arc::new(Folder {
            link_id: link.link_id.clone(),
            path,
            key,
            hash_key,
        }))
    }

    /// Ordnerinhalt (aus dem Cache oder frisch). Nicht entschlüsselbare Namen werden übersprungen.
    async fn children(
        &self,
        ctx: &Context<P>,
        folder: &Folder<P>,
        fresh: bool,
    ) -> CoreResult<Children> {
        if !fresh {
            if let Some(children) = self.cache().children.get(&folder.link_id) {
                return Ok(Arc::clone(children));
            }
        }
        let mut by_name: HashMap<String, Link> = HashMap::new();
        let mut undecryptable = 0;
        for page in 0.. {
            let response: ChildrenResponse = self
                .get(
                    "/drive/shares/{share}/folders/{link}/children",
                    format!(
                        "/drive/shares/{}/folders/{}/children?Page={page}&PageSize={CHILDREN_PAGE_SIZE}&ShowAll=0",
                        ctx.share_id, folder.link_id
                    ),
                )
                .await?;
            let count = response.links.len();
            for link in response.links {
                let Ok(name) = self.crypto.decrypt_name(&folder.key, &link.name) else {
                    undecryptable += 1;
                    continue;
                };
                let keep_existing = by_name
                    .get(&name)
                    .is_some_and(|existing| existing.is_active() && !link.is_active());
                if !keep_existing {
                    by_name.insert(name, link);
                }
            }
            if count < CHILDREN_PAGE_SIZE {
                break;
            }
        }
        if undecryptable > 0 {
            tracing::warn!(undecryptable, "skipped children with undecryptable names");
        }
        let children = Arc::new(by_name);
        let mut cache = self.cache();
        for (name, link) in children.iter() {
            if !link.is_folder() {
                if let Ok(path) = RemotePath::new(&join(&folder.path, name)) {
                    cache.file_by_link.insert(link.link_id.clone(), path);
                }
            }
        }
        cache
            .children
            .insert(folder.link_id.clone(), Arc::clone(&children));
        Ok(children)
    }

    fn forget_children(&self, folder_link_id: &str) {
        self.cache().children.remove(folder_link_id);
    }

    /// Unterordner `name` von `parent`; legt ihn mit `create` an.
    async fn child_folder(
        &self,
        ctx: &Context<P>,
        parent: &Folder<P>,
        name: &str,
        create: bool,
    ) -> CoreResult<Option<Arc<Folder<P>>>> {
        let path = join(&parent.path, name);
        if let Some(folder) = self.cache().folders.get(&path) {
            return Ok(Some(Arc::clone(folder)));
        }
        for fresh in [false, true] {
            let children = self.children(ctx, parent, fresh).await?;
            if let Some(link) = children
                .get(name)
                .filter(|l| l.is_folder() && l.is_active())
            {
                let folder = self.unlock_folder(&parent.key, link, path)?;
                self.cache().add_folder(Arc::clone(&folder));
                return Ok(Some(folder));
            }
        }
        if !create {
            return Ok(None);
        }
        match self.create_folder(ctx, parent, name, path.clone()).await {
            Ok(folder) => Ok(Some(folder)),
            // Parallel von einem anderen Gerät angelegt.
            Err(CoreError::Conflict(_)) => {
                let children = self.children(ctx, parent, true).await?;
                let link = children
                    .get(name)
                    .filter(|l| l.is_folder() && l.is_active())
                    .ok_or_else(|| {
                        CoreError::Conflict("name is taken by a file or draft".into())
                    })?;
                let folder = self.unlock_folder(&parent.key, link, path)?;
                self.cache().add_folder(Arc::clone(&folder));
                Ok(Some(folder))
            }
            Err(err) => Err(err),
        }
    }

    async fn create_folder(
        &self,
        ctx: &Context<P>,
        parent: &Folder<P>,
        name: &str,
        path: String,
    ) -> CoreResult<Arc<Folder<P>>> {
        let signer = ctx.signing_key();
        let node = self
            .crypto
            .new_node(&parent.key, &parent.hash_key, signer, name)?;
        let (hash_key, hash_key_armored) = self.crypto.new_folder_hash_key(&node.key)?;
        let body = json!({
            "ParentLinkID": parent.link_id,
            "Name": node.name,
            "Hash": node.hash,
            "NodeKey": node.node_key,
            "NodeHashKey": hash_key_armored,
            "NodePassphrase": node.node_passphrase,
            "NodePassphraseSignature": node.node_passphrase_signature,
            "SignatureAddress": ctx.address_email,
        });
        let result = self
            .post(
                "/drive/shares/{share}/folders",
                format!("/drive/shares/{}/folders", ctx.share_id),
                body,
            )
            .await;
        self.forget_children(&parent.link_id);
        let created: CreatedFolder = decode(result?, "create folder")?;
        let folder = Arc::new(Folder {
            link_id: created.folder.id,
            path,
            key: node.key,
            hash_key,
        });
        self.cache().add_folder(Arc::clone(&folder));
        Ok(folder)
    }

    /// Ordner zu `path` (relativ zu „Meine Dateien"), optional anlegen.
    async fn folder(
        &self,
        ctx: &Context<P>,
        path: &str,
        create: bool,
    ) -> CoreResult<Option<Arc<Folder<P>>>> {
        let mut current = Arc::clone(&ctx.root);
        if path.is_empty() {
            return Ok(Some(current));
        }
        for segment in path.split('/') {
            match self.child_folder(ctx, &current, segment, create).await? {
                Some(next) => current = next,
                None => return Ok(None),
            }
        }
        Ok(Some(current))
    }

    /// Aktive Datei unter `path`; bei Fehlschlag einmal frisch nachsehen.
    async fn find_file(
        &self,
        ctx: &Context<P>,
        path: &RemotePath,
    ) -> CoreResult<Option<(Arc<Folder<P>>, Link)>> {
        let (parent_path, name) = split(path);
        let Some(parent) = self.folder(ctx, parent_path, false).await? else {
            return Ok(None);
        };
        for fresh in [false, true] {
            let children = self.children(ctx, &parent, fresh).await?;
            if let Some(link) = children.get(name).filter(|l| l.active_revision().is_some()) {
                return Ok(Some((parent, link.clone())));
            }
        }
        Ok(None)
    }

    async fn download(
        &self,
        ctx: &Context<P>,
        parent: &Folder<P>,
        link: &Link,
    ) -> CoreResult<Vec<u8>> {
        let revision = link
            .active_revision()
            .ok_or_else(|| CoreError::NotFound("file has no active revision".into()))?;
        let packet = link
            .content_key_packet()
            .ok_or_else(|| CoreError::RemoteRejected("file without content key".into()))?;
        let node_key =
            self.crypto
                .unlock_node(&parent.key, &link.node_key, &link.node_passphrase)?;
        let session_key = self.crypto.session_key(&node_key, packet)?;

        let mut blocks = Vec::new();
        let mut from = 1;
        loop {
            let page: RevisionResponse = self
                .get(
                    "/drive/shares/{share}/files/{link}/revisions/{revision}",
                    format!(
                        "/drive/shares/{}/files/{}/revisions/{revision}?FromBlockIndex={from}&PageSize={BLOCKS_PAGE_SIZE}",
                        ctx.share_id, link.link_id
                    ),
                )
                .await?;
            let count = page.revision.blocks.len();
            from = page
                .revision
                .blocks
                .iter()
                .map(|b| b.index)
                .max()
                .unwrap_or(from)
                + 1;
            blocks.extend(page.revision.blocks);
            if count < BLOCKS_PAGE_SIZE {
                break;
            }
        }
        blocks.sort_by_key(|b| b.index);
        let mut content = Vec::new();
        for block in blocks {
            let token = crate::secret::Secret::new(block.token);
            let encrypted = self
                .session
                .api()
                .download_block(&block.bare_url, &token)
                .await?;
            content.extend(self.crypto.decrypt_block(&session_key, &encrypted)?);
        }
        Ok(content)
    }

    /// Lädt `bytes` als Inhalt der Entwurfs-Revision hoch und committet sie.
    #[allow(clippy::too_many_arguments)]
    async fn upload(
        &self,
        ctx: &Context<P>,
        link_id: &str,
        revision_id: &str,
        node_key: &PrivateKey<P>,
        session_key: &SessionKey<P>,
        bytes: &[u8],
    ) -> CoreResult<()> {
        let signer = ctx.signing_key();
        let verification: Verification = self
            .get(
                "/drive/v2/volumes/{volume}/links/{link}/revisions/{revision}/verification",
                format!(
                    "/drive/v2/volumes/{}/links/{link_id}/revisions/{revision_id}/verification",
                    ctx.volume_id
                ),
            )
            .await?;
        let code = STANDARD
            .decode(&verification.verification_code)
            .map_err(|_| CoreError::RemoteRejected("verification code is not base64".into()))?;

        let mut blocks = Vec::new();
        for chunk in bytes.chunks(self.block_size) {
            blocks.push(
                self.crypto
                    .encrypt_block(session_key, node_key, signer, chunk)?,
            );
        }
        if !blocks.is_empty() {
            let list: Vec<Value> = blocks
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    json!({
                        "Index": i + 1,
                        "Size": b.data.len(),
                        "EncSignature": b.enc_signature,
                        "Hash": STANDARD.encode(&b.hash),
                        "Verifier": { "Token": STANDARD.encode(verification_token(&code, &b.data)) },
                    })
                })
                .collect();
            let body = json!({
                "AddressID": ctx.address_id,
                "VolumeID": ctx.volume_id,
                "LinkID": link_id,
                "RevisionID": revision_id,
                "BlockList": list,
                "ThumbnailList": [],
            });
            let links: UploadLinks = decode(
                self.post("/drive/blocks", "/drive/blocks".into(), body)
                    .await?,
                "block upload",
            )?;
            for (i, block) in blocks.iter().enumerate() {
                let index = u32::try_from(i + 1).unwrap_or(u32::MAX);
                let target = links
                    .upload_links
                    .iter()
                    .find(|l| l.index == Some(index))
                    .or_else(|| links.upload_links.get(i))
                    .ok_or_else(|| CoreError::RemoteRejected("missing upload link".into()))?;
                let token = crate::secret::Secret::new(target.token.clone());
                self.session
                    .api()
                    .upload_block(&target.bare_url, &token, block.data.clone())
                    .await?;
            }
        }

        let manifest: Vec<u8> = blocks.iter().flat_map(|b| b.hash.iter().copied()).collect();
        let modified = chrono::DateTime::from_timestamp_millis(now().as_millis())
            .unwrap_or_default()
            .format("%Y-%m-%dT%H:%M:%S%z")
            .to_string();
        let xattr = json!({ "Common": {
            "ModificationTime": modified,
            "Size": bytes.len(),
            "BlockSizes": bytes.chunks(self.block_size).map(<[u8]>::len).collect::<Vec<_>>(),
            "Digests": { "SHA1": hex::encode(Sha1::digest(bytes)) },
        }});
        let commit = json!({
            "ManifestSignature": self.crypto.sign(signer, &manifest)?,
            "SignatureAddress": ctx.address_email,
            "ChecksumVerified": true,
            "XAttr": self.crypto.encrypt(node_key, xattr.to_string().as_bytes(), Some(signer))?,
        });
        self.session
            .send(
                Request::new(
                    Method::PUT,
                    "/drive/v2/volumes/{volume}/files/{link}/revisions/{revision}",
                    format!(
                        "/drive/v2/volumes/{}/files/{link_id}/revisions/{revision_id}",
                        ctx.volume_id
                    ),
                )
                .json(commit),
            )
            .await?;
        Ok(())
    }

    /// Entwurfs-Links endgültig löschen (gilt nur für Entwürfe).
    async fn delete_drafts(
        &self,
        ctx: &Context<P>,
        parent: &Folder<P>,
        link_id: &str,
    ) -> CoreResult<()> {
        let body = self
            .post(
                "/drive/shares/{share}/folders/{link}/delete_multiple",
                format!(
                    "/drive/shares/{}/folders/{}/delete_multiple",
                    ctx.share_id, parent.link_id
                ),
                json!({ "LinkIDs": [link_id] }),
            )
            .await?;
        check_multi(body, "delete draft")
    }

    async fn delete_revision(
        &self,
        ctx: &Context<P>,
        link_id: &str,
        revision_id: &str,
    ) -> CoreResult<()> {
        self.session
            .send(Request::new(
                Method::DELETE,
                "/drive/shares/{share}/files/{link}/revisions/{revision}",
                format!(
                    "/drive/shares/{}/files/{link_id}/revisions/{revision_id}",
                    ctx.share_id
                ),
            ))
            .await
            .map(drop)
    }

    async fn create_file(
        &self,
        ctx: &Context<P>,
        parent: &Folder<P>,
        path: &RemotePath,
        bytes: Vec<u8>,
    ) -> CoreResult<RemoteEntry> {
        let (_, name) = split(path);
        let signer = ctx.signing_key();
        let mut removed_stale_draft = false;
        loop {
            let node = self
                .crypto
                .new_node(&parent.key, &parent.hash_key, signer, name)?;
            let content = self.crypto.new_content_key(&node.key)?;
            let body = json!({
                "ParentLinkID": parent.link_id,
                "Name": node.name,
                "Hash": node.hash,
                "NodeKey": node.node_key,
                "NodePassphrase": node.node_passphrase,
                "NodePassphraseSignature": node.node_passphrase_signature,
                "MIMEType": mime_type(name),
                "ContentKeyPacket": content.key_packet_b64,
                "ContentKeyPacketSignature": content.key_packet_signature,
                "SignatureAddress": ctx.address_email,
                "ClientUID": self.client_uid,
            });
            let result = self
                .session
                .try_send(
                    Request::post(
                        "/drive/shares/{share}/files",
                        format!("/drive/shares/{}/files", ctx.share_id),
                    )
                    .json(body),
                )
                .await?;
            self.forget_children(&parent.link_id);
            let created: CreatedFile = match result {
                Ok(value) => decode(value, "create file")?,
                Err(err) => {
                    let Some(details) = conflict_details(&err) else {
                        return Err(err.into());
                    };
                    // Liegengebliebener Entwurf (abgebrochener Upload): einmal entfernen und neu
                    // versuchen. Eine aktive Datei gleichen Namens ist ein echter Konflikt.
                    match (
                        details.link_id,
                        details.revision_id,
                        details.draft_revision_id,
                    ) {
                        (Some(draft_link), None, Some(_)) if !removed_stale_draft => {
                            tracing::info!("removing stale draft before upload");
                            self.delete_drafts(ctx, parent, &draft_link).await?;
                            removed_stale_draft = true;
                            continue;
                        }
                        _ => return Err(CoreError::Conflict(format!("{path} exists"))),
                    }
                }
            };
            let uploaded = self
                .upload(
                    ctx,
                    &created.file.id,
                    &created.file.revision_id,
                    &node.key,
                    &content.session_key,
                    &bytes,
                )
                .await;
            if let Err(err) = uploaded {
                // Entwurf nicht liegen lassen (scheitert harmlos, falls der Commit doch durchging).
                if let Err(cleanup) = self.delete_drafts(ctx, parent, &created.file.id).await {
                    tracing::debug!(error = %cleanup, "draft cleanup failed");
                }
                return Err(err);
            }
            self.forget_children(&parent.link_id);
            return Ok(RemoteEntry {
                path: path.clone(),
                size: bytes.len() as u64,
                revision: Revision(created.file.revision_id),
                modified: now(),
            });
        }
    }

    async fn replace_file(
        &self,
        ctx: &Context<P>,
        parent: &Folder<P>,
        link: &Link,
        path: &RemotePath,
        bytes: Vec<u8>,
    ) -> CoreResult<RemoteEntry> {
        let current = link
            .active_revision()
            .ok_or_else(|| CoreError::NotFound(path.to_string()))?
            .to_owned();
        let packet = link
            .content_key_packet()
            .ok_or_else(|| CoreError::RemoteRejected("file without content key".into()))?;
        let node_key =
            self.crypto
                .unlock_node(&parent.key, &link.node_key, &link.node_passphrase)?;
        let session_key = self.crypto.session_key(&node_key, packet)?;

        let mut removed_stale_draft = false;
        let revision_id = loop {
            let result = self
                .session
                .try_send(
                    Request::post(
                        "/drive/shares/{share}/files/{link}/revisions",
                        format!(
                            "/drive/shares/{}/files/{}/revisions",
                            ctx.share_id, link.link_id
                        ),
                    )
                    .json(json!({ "CurrentRevisionID": current, "ClientUID": self.client_uid })),
                )
                .await?;
            match result {
                Ok(value) => {
                    break decode::<CreatedRevision>(value, "create revision")?
                        .revision
                        .id
                }
                Err(err) => {
                    let Some(details) = conflict_details(&err) else {
                        return Err(err.into());
                    };
                    match details.draft_revision_id {
                        // Nur dieses Gerät schreibt die Datei: offener Entwurf = eigener Abbruch.
                        Some(draft) if !removed_stale_draft => {
                            self.delete_revision(ctx, &link.link_id, &draft).await?;
                            removed_stale_draft = true;
                        }
                        _ => {
                            self.forget_children(&parent.link_id);
                            return Err(CoreError::Conflict(format!("{path}: newer revision")));
                        }
                    }
                }
            }
        };
        let uploaded = self
            .upload(
                ctx,
                &link.link_id,
                &revision_id,
                &node_key,
                &session_key,
                &bytes,
            )
            .await;
        self.forget_children(&parent.link_id);
        if let Err(err) = uploaded {
            if let Err(cleanup) = self.delete_revision(ctx, &link.link_id, &revision_id).await {
                tracing::debug!(error = %cleanup, "draft revision cleanup failed");
            }
            return Err(err);
        }
        // Alte Revision aufräumen (Metadateien werden oft ersetzt).
        if let Err(err) = self.delete_revision(ctx, &link.link_id, &current).await {
            tracing::debug!(error = %err, "obsolete revision not deleted");
        }
        Ok(RemoteEntry {
            path: path.clone(),
            size: bytes.len() as u64,
            revision: Revision(revision_id),
            modified: now(),
        })
    }

    /// Ordner zu einer Link-ID, ggf. über die Elternkette nachgeladen. `None` = außerhalb.
    async fn resolve_folder(
        &self,
        ctx: &Context<P>,
        link_id: &str,
    ) -> CoreResult<Option<Arc<Folder<P>>>> {
        // Unbekannte Vorfahren sammeln, bis ein bekannter Ordner erreicht ist.
        let mut chain: Vec<Link> = Vec::new();
        let mut next = link_id.to_owned();
        let known = loop {
            if let Some(folder) = self.cache().folder_for_link(&next) {
                break folder;
            }
            if chain.len() >= MAX_RESOLVE_DEPTH {
                return Ok(None);
            }
            let response: CoreResult<LinkResponse> = self
                .get(
                    "/drive/shares/{share}/links/{link}",
                    format!("/drive/shares/{}/links/{next}", ctx.share_id),
                )
                .await;
            let link = match response {
                Ok(r) => r.link,
                Err(CoreError::NotFound(_)) => return Ok(None),
                Err(err) => return Err(err),
            };
            if !link.is_folder() || !link.is_active() {
                return Ok(None);
            }
            let Some(parent) = link.parent_link_id.clone() else {
                return Ok(None);
            };
            chain.push(link);
            next = parent;
        };
        let mut folder = known;
        for link in chain.into_iter().rev() {
            let name = self.crypto.decrypt_name(&folder.key, &link.name)?;
            let child = self.unlock_folder(&folder.key, &link, join(&folder.path, &name))?;
            self.cache().add_folder(Arc::clone(&child));
            folder = child;
        }
        Ok(Some(folder))
    }

    /// Testordner samt Inhalt in den Papierkorb und endgültig löschen (nur Live-Tests).
    #[cfg(test)]
    pub(crate) async fn remove_folder(&self, path: &str) -> CoreResult<()> {
        let ctx = self.context().await?;
        let (parent_path, _) = path.rsplit_once('/').unwrap_or(("", path));
        let (Some(parent), Some(folder)) = (
            self.folder(&ctx, parent_path, false).await?,
            self.folder(&ctx, path, false).await?,
        ) else {
            return Ok(());
        };
        let ids = json!({ "LinkIDs": [folder.link_id] });
        let trashed = self
            .post(
                "/drive/shares/{share}/folders/{link}/trash_multiple",
                format!(
                    "/drive/shares/{}/folders/{}/trash_multiple",
                    ctx.share_id, parent.link_id
                ),
                ids.clone(),
            )
            .await?;
        check_multi(trashed, "trash folder")?;
        let purged = self
            .post(
                "/drive/v2/volumes/{volume}/trash/delete_multiple",
                format!("/drive/v2/volumes/{}/trash/delete_multiple", ctx.volume_id),
                ids,
            )
            .await?;
        check_multi(purged, "delete folder from trash")
    }

    async fn latest_event(&self, ctx: &Context<P>) -> CoreResult<RemoteCursor> {
        let latest: LatestEvent = self
            .get(
                "/drive/volumes/{volume}/events/latest",
                format!("/drive/volumes/{}/events/latest", ctx.volume_id),
            )
            .await?;
        Ok(RemoteCursor(latest.event_id))
    }

    async fn apply_event(
        &self,
        ctx: &Context<P>,
        event_type: u8,
        raw: Value,
        changes: &mut RemoteChanges,
    ) -> CoreResult<()> {
        if event_type == EVENT_DELETE {
            let link_id = raw
                .get("LinkID")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut cache = self.cache();
            // Elternordner unbekannt → alle Ordnerinhalte neu laden.
            cache.children.clear();
            if let Some(path) = cache.file_by_link.remove(link_id) {
                if cache.in_roots(path.as_str()) {
                    changes.deleted.push(path);
                }
            }
            return Ok(());
        }
        let Ok(mut link) = serde_json::from_value::<Link>(raw) else {
            return Ok(());
        };
        let Some(parent_id) = link.parent_link_id.as_deref() else {
            return Ok(());
        };
        self.forget_children(parent_id);
        if link.is_folder() {
            return Ok(());
        }
        let Some(parent) = self.resolve_folder(ctx, parent_id).await? else {
            return Ok(());
        };
        if !self.cache().in_roots(&parent.path) {
            return Ok(());
        }
        let Ok(name) = self.crypto.decrypt_name(&parent.key, &link.name) else {
            return Ok(());
        };
        let path = RemotePath::new(&join(&parent.path, &name))?;
        if link.is_draft() {
            // Event beim Anlegen des Entwurfs; ob danach ein eigenes Commit-Event kommt, ist nicht
            // dokumentiert – daher den aktuellen Stand nachladen.
            match self
                .get::<LinkResponse>(
                    "/drive/shares/{share}/links/{link}",
                    format!("/drive/shares/{}/links/{}", ctx.share_id, link.link_id),
                )
                .await
            {
                Ok(fresh) => link = fresh.link,
                Err(CoreError::NotFound(_)) => return Ok(()),
                Err(err) => return Err(err),
            }
        }
        if let Some(entry) = link.entry(path.clone()) {
            self.cache().file_by_link.insert(link.link_id.clone(), path);
            changes.changed.push(entry);
        } else if !link.is_draft() {
            // Papierkorb (Update mit State 2 / Trashed).
            self.cache().file_by_link.remove(&link.link_id);
            changes.deleted.push(path);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<P: Pgp> RemoteStore for Drive<P> {
    async fn open_app_root(&self, name: &str) -> CoreResult<RemoteDir> {
        let path = RemotePath::new(name)?;
        let ctx = self.context().await?;
        self.folder(&ctx, path.as_str(), true).await?;
        self.cache().roots.insert(path.as_str().to_owned());
        Ok(RemoteDir { path })
    }

    async fn list(
        &self,
        dir: &RemotePath,
        _since: Option<&RemoteCursor>,
    ) -> CoreResult<RemoteListing> {
        let ctx = self.context().await?;
        let mut entries = Vec::new();
        let mut pending = Vec::new();
        if let Some(folder) = self.folder(&ctx, dir.as_str(), false).await? {
            pending.push(folder);
        }
        while let Some(folder) = pending.pop() {
            let children = self.children(&ctx, &folder, true).await?;
            for (name, link) in children.iter() {
                let path = join(&folder.path, name);
                if link.is_folder() {
                    if link.is_active() {
                        let child = self.unlock_folder(&folder.key, link, path)?;
                        self.cache().add_folder(Arc::clone(&child));
                        pending.push(child);
                    }
                } else if let Some(entry) = link.entry(RemotePath::new(&path)?) {
                    entries.push(entry);
                }
            }
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(RemoteListing {
            entries,
            cursor: RemoteCursor(String::new()),
        })
    }

    async fn read(&self, path: &RemotePath) -> CoreResult<Vec<u8>> {
        let ctx = self.context().await?;
        let Some((parent, link)) = self.find_file(&ctx, path).await? else {
            return Err(CoreError::NotFound(path.to_string()));
        };
        match self.download(&ctx, &parent, &link).await {
            Err(CoreError::NotFound(_)) => {
                self.forget_children(&parent.link_id);
                Err(CoreError::NotFound(path.to_string()))
            }
            result => result,
        }
    }

    async fn create(&self, path: &RemotePath, bytes: Vec<u8>) -> CoreResult<RemoteEntry> {
        let ctx = self.context().await?;
        let (parent_path, _) = split(path);
        let parent = self
            .folder(&ctx, parent_path, true)
            .await?
            .ok_or_else(|| CoreError::Internal("parent folder not created".into()))?;
        self.create_file(&ctx, &parent, path, bytes).await
    }

    async fn replace(
        &self,
        path: &RemotePath,
        bytes: Vec<u8>,
        expected: Option<&Revision>,
    ) -> CoreResult<RemoteEntry> {
        let ctx = self.context().await?;
        let (parent_path, name) = split(path);
        // Immer den aktuellen Stand vom Server (Optimistic Concurrency).
        let existing = match self.folder(&ctx, parent_path, false).await? {
            Some(parent) => {
                let children = self.children(&ctx, &parent, true).await?;
                children
                    .get(name)
                    .filter(|l| l.active_revision().is_some())
                    .cloned()
                    .map(|link| (parent, link))
            }
            None => None,
        };
        match (existing, expected) {
            (None, Some(_)) => Err(CoreError::Conflict(format!("{path} does not exist"))),
            (None, None) => self.create(path, bytes).await,
            (Some((_, link)), Some(expected)) if link.active_revision() != Some(&expected.0) => {
                Err(CoreError::Conflict(format!("{path}: revision changed")))
            }
            (Some((parent, link)), _) => self.replace_file(&ctx, &parent, &link, path, bytes).await,
        }
    }

    async fn delete(&self, path: &RemotePath) -> CoreResult<()> {
        let ctx = self.context().await?;
        let Some((parent, link)) = self.find_file(&ctx, path).await? else {
            return Ok(());
        };
        let ids = json!({ "LinkIDs": [link.link_id] });
        let trashed = self
            .post(
                "/drive/shares/{share}/folders/{link}/trash_multiple",
                format!(
                    "/drive/shares/{}/folders/{}/trash_multiple",
                    ctx.share_id, parent.link_id
                ),
                ids.clone(),
            )
            .await;
        self.forget_children(&parent.link_id);
        check_multi(trashed?, "trash")?;
        // Endgültig löschen, damit der Name frei wird und der Papierkorb nicht volläuft.
        let purged = self
            .post(
                "/drive/v2/volumes/{volume}/trash/delete_multiple",
                format!("/drive/v2/volumes/{}/trash/delete_multiple", ctx.volume_id),
                ids,
            )
            .await
            .and_then(|body| check_multi(body, "delete from trash"));
        if let Err(err) = purged {
            tracing::warn!(error = %err, "file stays in trash");
        }
        self.cache().file_by_link.remove(&link.link_id);
        Ok(())
    }

    async fn changes_since(
        &self,
        cursor: Option<&RemoteCursor>,
    ) -> CoreResult<Option<RemoteChanges>> {
        let ctx = self.context().await?;
        let Some(cursor) = cursor else {
            return Ok(Some(RemoteChanges {
                changed: Vec::new(),
                deleted: Vec::new(),
                cursor: self.latest_event(&ctx).await?,
                refresh: false,
            }));
        };
        let mut changes = RemoteChanges {
            changed: Vec::new(),
            deleted: Vec::new(),
            cursor: cursor.clone(),
            refresh: false,
        };
        for _ in 0..MAX_EVENT_PAGES {
            let response: CoreResult<Events> = self
                .get(
                    "/drive/volumes/{volume}/events/{event}",
                    format!(
                        "/drive/volumes/{}/events/{}",
                        ctx.volume_id, changes.cursor.0
                    ),
                )
                .await;
            let events = match response {
                Ok(events) => events,
                // Cursor unbekannt (zu alt): Vollabgleich.
                Err(CoreError::NotFound(_)) => {
                    return Ok(Some(RemoteChanges {
                        changed: Vec::new(),
                        deleted: Vec::new(),
                        cursor: self.latest_event(&ctx).await?,
                        refresh: true,
                    }))
                }
                Err(err) => return Err(err),
            };
            if events.refresh != 0 {
                self.cache().children.clear();
                return Ok(Some(RemoteChanges {
                    changed: Vec::new(),
                    deleted: Vec::new(),
                    cursor: self.latest_event(&ctx).await?,
                    refresh: true,
                }));
            }
            for event in events.events {
                self.apply_event(&ctx, event.event_type, event.link, &mut changes)
                    .await?;
            }
            changes.cursor = RemoteCursor(events.event_id);
            if events.more == 0 {
                break;
            }
        }
        // Je Pfad nur der letzte Stand; erst gelöscht, dann neu angelegt zählt als vorhanden.
        let mut latest: HashMap<RemotePath, RemoteEntry> = HashMap::new();
        for entry in changes.changed.drain(..) {
            latest.insert(entry.path.clone(), entry);
        }
        changes.changed = latest.into_values().collect();
        changes.changed.sort_by(|a, b| a.path.cmp(&b.path));
        let changed: HashSet<&RemotePath> = changes.changed.iter().map(|e| &e.path).collect();
        let deleted: Vec<RemotePath> = changes
            .deleted
            .iter()
            .filter(|p| !changed.contains(p))
            .cloned()
            .collect();
        changes.deleted = deleted;
        Ok(Some(changes))
    }
}
