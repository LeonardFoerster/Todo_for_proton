//! Zustandsbehafteter Fake-Server für Proton Drive (nur Tests).
//!
//! Bildet die vom Store genutzten Endpunkte nach, mit dem Verhalten aus den P2-Proben und der
//! SDK-Referenz: Namenskonflikt über `Hash` (2500 + `Details`), Entwürfe, Revisionen,
//! Block-Verifikation (`Verifier.Token` wird geprüft), Papierkorb und Volume-Events mit `More`.
//! Der Server sieht – wie Proton – nur verschlüsselte Namen und Inhalte.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use proton_crypto::crypto::{DataEncoding, KeyGenerator, KeyGeneratorSync, PGPProviderSync};
use serde_json::{json, Value};
use wiremock::{Request, Respond, ResponseTemplate};

use super::crypto::{random, verification_token, DriveCrypto};

pub(crate) const KEY_PASSWORD: &str = "kp-1";
pub(crate) const VOLUME: &str = "vol-1";
pub(crate) const SHARE: &str = "share-1";
pub(crate) const ROOT: &str = "root";
const EVENT_PAGE: usize = 3;

#[derive(Clone)]
struct FakeLink {
    json: Value,
    parent: Option<String>,
    hash: String,
}

struct FakeRevision {
    link: String,
    /// 0 Entwurf, 1 aktiv, 2 veraltet.
    state: u8,
    code: Vec<u8>,
    blocks: BTreeMap<u32, String>,
    sizes: u64,
    client_uid: Option<String>,
}

/// Wie der Server auf den nächsten passenden Aufruf reagiert.
#[derive(Clone)]
pub(crate) enum Fault {
    /// Antwort ersetzen, Aufruf nicht ausführen.
    Respond(u16, Value),
    /// Aufruf ausführen, dann trotzdem mit Fehler antworten (verlorene Antwort).
    AfterApply(u16),
}

#[derive(Default)]
struct State {
    links: HashMap<String, FakeLink>,
    revisions: HashMap<String, FakeRevision>,
    blobs: HashMap<String, Vec<u8>>,
    /// Upload-Token → (Revision, Index, erwarteter Verifier).
    uploads: HashMap<String, (String, u32, Vec<u8>)>,
    events: Vec<(u64, Value)>,
    next: u64,
    faults: Vec<(String, String, Fault)>,
    calls: Vec<String>,
    origin: String,
    static_json: HashMap<&'static str, Value>,
}

impl State {
    fn id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}-{}", self.next)
    }

    fn event(&mut self, event_type: u8, link: Value) {
        self.next += 1;
        let id = self.next;
        let mut event = json!({ "EventID": id.to_string(), "EventType": event_type });
        event["Link"] = link;
        self.events.push((id, event));
    }

    fn conflict(&self, parent: &str, hash: &str) -> Option<Value> {
        let (id, existing) = self
            .links
            .iter()
            .find(|(_, l)| l.parent.as_deref() == Some(parent) && l.hash == hash)?;
        let draft = self
            .revisions
            .iter()
            .find(|(_, r)| r.link == *id && r.state == 0);
        let active = existing.json["FileProperties"]["ActiveRevision"]["ID"].clone();
        Some(json!({
            "Code": 2500,
            "Error": "A file or folder with that name already exists",
            "Details": {
                "ConflictLinkID": id,
                "ConflictRevisionID": active,
                "ConflictDraftRevisionID": draft.map(|(rid, _)| rid.clone()),
                "ConflictDraftClientUID": draft.and_then(|(_, r)| r.client_uid.clone()),
            }
        }))
    }
}

/// Der Fake-Server. Klonbar; alle Klone teilen den Zustand.
#[derive(Clone)]
pub(crate) struct FakeDrive {
    state: Arc<Mutex<State>>,
}

fn ok(mut body: Value) -> ResponseTemplate {
    body["Code"] = json!(1000);
    ResponseTemplate::new(200).set_body_json(body)
}

fn error(status: u16, code: i64) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({ "Code": code, "Error": "fake error" }))
}

fn now_s() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// Inhalt des Multipart-Felds `Block` (einfacher Parser für reqwests Format).
fn multipart_block(body: &[u8]) -> Vec<u8> {
    let start = body
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("multipart headers")
        + 4;
    let end = body
        .windows(4)
        .rposition(|w| w == b"\r\n--")
        .expect("multipart end");
    body[start..end].to_vec()
}

impl FakeDrive {
    /// Konto mit User-, Adress-, Share-Key und Wurzelordner, alles lokal erzeugt.
    pub(crate) fn new() -> Self {
        let p = proton_crypto::new_pgp_provider();
        let crypto = DriveCrypto::new(p.clone());
        let generate = |name: &str| {
            p.new_key_generator()
                .with_user_id(name, "fake@example.org")
                .generate()
                .unwrap()
        };
        let export = |key, pass: &[u8]| {
            String::from_utf8(
                p.private_key_export(key, pass, DataEncoding::Armor)
                    .unwrap()
                    .as_ref()
                    .to_vec(),
            )
            .unwrap()
        };
        let user = generate("user");
        let address = generate("address");
        let share = generate("share");
        let share_pass = STANDARD.encode(random::<32>().unwrap());

        let root = crypto.new_node(&share, &[0; 32], &address, "root").unwrap();
        let (_, root_hash_key) = crypto.new_folder_hash_key(&root.key).unwrap();

        let mut state = State::default();
        state.static_json.insert(
            "users",
            json!({ "User": { "ID": "user-1", "Name": "fake", "Email": "fake@example.org", "Keys": [{
                "ID": "uk-1", "Version": 3, "PrivateKey": export(&user, KEY_PASSWORD.as_bytes()),
                "Token": null, "Signature": null, "Activation": null, "Primary": 1, "Active": 1,
                "Flags": null, "RecoverySecret": null, "RecoverySecretSignature": null,
                "AddressForwardingID": null,
            }]}}),
        );
        state.static_json.insert(
            "addresses",
            json!({ "Addresses": [{ "ID": "addr-1", "Email": "fake@example.org", "Keys": [{
                "ID": "ak-1", "Version": 3, "PrivateKey": export(&address, KEY_PASSWORD.as_bytes()),
                "Token": null, "Signature": null, "Activation": null, "Primary": 1, "Active": 1,
                "Flags": 3, "RecoverySecret": null, "RecoverySecretSignature": null,
                "AddressForwardingID": null,
            }]}]}),
        );
        state.static_json.insert(
            "share",
            json!({
                "ShareID": SHARE, "VolumeID": VOLUME, "LinkID": ROOT, "AddressID": "addr-1",
                "Key": export(&share, share_pass.as_bytes()),
                "Passphrase": crypto.encrypt(&address, share_pass.as_bytes(), None).unwrap(),
                "PassphraseSignature": crypto.sign(&address, share_pass.as_bytes()).unwrap(),
            }),
        );
        state.links.insert(
            ROOT.into(),
            FakeLink {
                json: json!({
                    "LinkID": ROOT, "ParentLinkID": null, "Type": 1, "Name": root.name,
                    "Hash": null, "State": 1, "Size": 0, "ModifyTime": now_s(), "Trashed": null,
                    "NodeKey": root.node_key, "NodePassphrase": root.node_passphrase,
                    "NodePassphraseSignature": root.node_passphrase_signature,
                    "FileProperties": null,
                    "FolderProperties": { "NodeHashKey": root_hash_key },
                }),
                parent: None,
                hash: String::new(),
            },
        );
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    /// Basis-URL des Servers (für Storage-URLs).
    pub(crate) fn set_origin(&self, origin: String) {
        self.state().origin = origin;
    }

    /// Nächster Aufruf `method` mit Pfad, der `path_part` enthält, bekommt `fault`.
    pub(crate) fn fail_once(&self, method: &str, path_part: &str, fault: Fault) {
        self.state()
            .faults
            .push((method.to_owned(), path_part.to_owned(), fault));
    }

    /// Bisherige Aufrufe als `METHOD /pfad`.
    pub(crate) fn calls(&self) -> Vec<String> {
        self.state().calls.clone()
    }

    pub(crate) fn count_calls(&self, method: &str, path_part: &str) -> usize {
        let prefix = format!("{method} ");
        self.calls()
            .iter()
            .filter(|c| c.starts_with(&prefix) && c.contains(path_part))
            .count()
    }

    /// Links (ohne Wurzel) nach Zustand: (Entwürfe, aktiv, Papierkorb).
    pub(crate) fn link_states(&self) -> (usize, usize, usize) {
        let state = self.state();
        let count = |s: i64| {
            state
                .links
                .iter()
                .filter(|(id, l)| id.as_str() != ROOT && l.json["State"] == json!(s))
                .count()
        };
        (count(0), count(1), count(2))
    }

    /// Nicht-entwurfs Revisionen (aktiv + veraltet) aller Dateien.
    pub(crate) fn committed_revisions(&self) -> usize {
        self.state()
            .revisions
            .values()
            .filter(|r| r.state != 0)
            .count()
    }

    /// Alles, was der Server gespeichert hat (Links, Blöcke), als Text.
    pub(crate) fn stored_text(&self) -> String {
        let state = self.state();
        let mut text: String = state.links.values().map(|l| l.json.to_string()).collect();
        for blob in state.blobs.values() {
            text.push_str(&String::from_utf8_lossy(blob));
        }
        text
    }

    /// Legt einen Ordner/eine Datei außerhalb des App-Ordners an (für Event-Filter-Tests).
    pub(crate) fn touch_root_event(&self) {
        let mut state = self.state();
        let id = state.id("foreign");
        let link = json!({ "LinkID": id, "ParentLinkID": ROOT, "Type": 2, "Name": "x", "State": 1,
            "NodeKey": "k", "NodePassphrase": "p", "FileProperties": null, "FolderProperties": null });
        state.event(1, link);
    }

    fn handle(&self, req: &Request) -> ResponseTemplate {
        let method = req.method.as_str().to_owned();
        let path = req.url.path().to_owned();
        let query: HashMap<String, String> = req.url.query_pairs().into_owned().collect();
        let mut state = self.state();
        state.calls.push(format!("{method} {path}"));
        let fault = state
            .faults
            .iter()
            .position(|(m, p, _)| *m == method && path.contains(p.as_str()))
            .map(|i| state.faults.remove(i).2);
        if let Some(Fault::Respond(status, body)) = &fault {
            return ResponseTemplate::new(*status).set_body_json(body.clone());
        }
        let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let origin = state.origin.clone();
        let response = route(&mut state, &method, &segments, &query, &body, req, &origin);
        match fault {
            Some(Fault::AfterApply(status)) => error(status, 0),
            _ => response,
        }
    }
}

impl Respond for FakeDrive {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.handle(request)
    }
}

#[allow(clippy::too_many_lines)]
fn route(
    state: &mut State,
    method: &str,
    segments: &[&str],
    query: &HashMap<String, String>,
    body: &Value,
    req: &Request,
    origin: &str,
) -> ResponseTemplate {
    match (method, segments) {
        ("POST", ["auth", "v4", "refresh"]) => ok(json!({
            "UID": "uid-1", "AccessToken": "acc", "RefreshToken": "ref", "TokenType": "Bearer",
        })),
        ("GET", ["core", "v4", "users"]) => ok(state.static_json["users"].clone()),
        ("GET", ["core", "v4", "addresses"]) => ok(state.static_json["addresses"].clone()),
        ("GET", ["drive", "volumes"]) => ok(json!({ "Volumes": [{
            "VolumeID": VOLUME, "State": 1, "Share": { "ShareID": SHARE, "LinkID": ROOT },
        }]})),
        ("GET", ["drive", "shares", SHARE]) => ok(state.static_json["share"].clone()),
        ("GET", ["drive", "shares", SHARE, "links", id]) => match state.links.get(*id) {
            Some(link) => ok(json!({ "Link": link.json })),
            None => error(422, 2501),
        },
        ("GET", ["drive", "shares", SHARE, "folders", id, "children"]) => {
            let page: usize = query["Page"].parse().unwrap();
            let size: usize = query["PageSize"].parse().unwrap();
            let mut children: Vec<&FakeLink> = state
                .links
                .values()
                .filter(|l| l.parent.as_deref() == Some(*id) && l.json["State"] != json!(0))
                .collect();
            children.sort_by_key(|l| l.json["LinkID"].as_str().unwrap_or_default().to_owned());
            let links: Vec<Value> = children
                .into_iter()
                .skip(page * size)
                .take(size)
                .map(|l| l.json.clone())
                .collect();
            ok(json!({ "Links": links }))
        }
        ("POST", ["drive", "shares", SHARE, "folders"]) => {
            let parent = body["ParentLinkID"].as_str().unwrap().to_owned();
            let hash = body["Hash"].as_str().unwrap().to_owned();
            if let Some(conflict) = state.conflict(&parent, &hash) {
                return ResponseTemplate::new(422).set_body_json(conflict);
            }
            let id = state.id("folder");
            let json = json!({
                "LinkID": id, "ParentLinkID": parent, "Type": 1, "Name": body["Name"],
                "Hash": hash, "State": 1, "Size": 0, "ModifyTime": now_s(), "Trashed": null,
                "NodeKey": body["NodeKey"], "NodePassphrase": body["NodePassphrase"],
                "NodePassphraseSignature": body["NodePassphraseSignature"],
                "FileProperties": null,
                "FolderProperties": { "NodeHashKey": body["NodeHashKey"] },
            });
            state.links.insert(
                id.clone(),
                FakeLink {
                    json: json.clone(),
                    parent: Some(parent),
                    hash,
                },
            );
            state.event(1, json);
            ok(json!({ "Folder": { "ID": id } }))
        }
        ("POST", ["drive", "shares", SHARE, "files"]) => {
            let parent = body["ParentLinkID"].as_str().unwrap().to_owned();
            let hash = body["Hash"].as_str().unwrap().to_owned();
            if let Some(conflict) = state.conflict(&parent, &hash) {
                return ResponseTemplate::new(422).set_body_json(conflict);
            }
            let id = state.id("file");
            let rev = state.id("rev");
            let json = json!({
                "LinkID": id, "ParentLinkID": parent, "Type": 2, "Name": body["Name"],
                "Hash": hash, "State": 0, "Size": 0, "ModifyTime": now_s(), "Trashed": null,
                "MIMEType": body["MIMEType"],
                "NodeKey": body["NodeKey"], "NodePassphrase": body["NodePassphrase"],
                "NodePassphraseSignature": body["NodePassphraseSignature"],
                "FileProperties": { "ContentKeyPacket": body["ContentKeyPacket"], "ActiveRevision": null },
                "FolderProperties": null,
            });
            state.links.insert(
                id.clone(),
                FakeLink {
                    json: json.clone(),
                    parent: Some(parent),
                    hash,
                },
            );
            // Wie live beobachtet: Create-Event schon für den Entwurf.
            state.event(1, json);
            let client_uid = body["ClientUID"].as_str().map(str::to_owned);
            state.revisions.insert(
                rev.clone(),
                FakeRevision {
                    link: id.clone(),
                    state: 0,
                    code: random::<32>().unwrap().to_vec(),
                    blocks: BTreeMap::new(),
                    sizes: 0,
                    client_uid: client_uid.clone(),
                },
            );
            ok(json!({ "File": { "ID": id, "RevisionID": rev, "ClientUID": client_uid } }))
        }
        ("POST", ["drive", "shares", SHARE, "files", link, "revisions"]) => {
            let Some(file) = state.links.get(*link) else {
                return error(422, 2501);
            };
            let active = file.json["FileProperties"]["ActiveRevision"]["ID"].clone();
            if let Some((draft, _)) = state
                .revisions
                .iter()
                .find(|(_, r)| r.link == *link && r.state == 0)
            {
                return ResponseTemplate::new(409).set_body_json(json!({
                    "Code": 2500, "Error": "draft exists",
                    "Details": { "ConflictLinkID": link, "ConflictDraftRevisionID": draft },
                }));
            }
            if body["CurrentRevisionID"] != active {
                return error(409, 2500);
            }
            let rev = state.id("rev");
            state.revisions.insert(
                rev.clone(),
                FakeRevision {
                    link: (*link).to_owned(),
                    state: 0,
                    code: random::<32>().unwrap().to_vec(),
                    blocks: BTreeMap::new(),
                    sizes: 0,
                    client_uid: body["ClientUID"].as_str().map(str::to_owned),
                },
            );
            ok(json!({ "Revision": { "ID": rev } }))
        }
        ("DELETE", ["drive", "shares", SHARE, "files", _, "revisions", rev]) => {
            match state.revisions.get(*rev) {
                None => error(422, 2501),
                Some(r) if r.state == 1 => error(422, 2511),
                Some(_) => {
                    state.revisions.remove(*rev);
                    ok(json!({}))
                }
            }
        }
        (
            "GET",
            ["drive", "v2", "volumes", VOLUME, "links", link, "revisions", rev, "verification"],
        ) => match state.revisions.get(*rev).filter(|r| r.link == *link) {
            Some(r) => ok(json!({
                "VerificationCode": STANDARD.encode(&r.code),
                "ContentKeyPacket": state.links[*link].json["FileProperties"]["ContentKeyPacket"],
            })),
            None => error(422, 2501),
        },
        ("POST", ["drive", "blocks"]) => {
            let rev = body["RevisionID"].as_str().unwrap().to_owned();
            let Some(code) = state.revisions.get(&rev).map(|r| r.code.clone()) else {
                return error(422, 2501);
            };
            let mut links = Vec::new();
            for block in body["BlockList"].as_array().unwrap() {
                let index = u32::try_from(block["Index"].as_u64().unwrap()).unwrap();
                let Some(token) = block["Verifier"]["Token"].as_str() else {
                    return error(400, 2000);
                };
                let token = STANDARD.decode(token).unwrap();
                let upload = state.id("upload");
                state
                    .uploads
                    .insert(upload.clone(), (rev.clone(), index, token));
                links.push(json!({ "Index": index, "Token": upload, "BareURL": format!("{origin}/storage/{upload}") }));
            }
            let _ = code;
            ok(json!({ "UploadLinks": links, "ThumbnailLinks": [] }))
        }
        ("POST", ["storage", upload]) => {
            let token = req
                .headers
                .get("pm-storage-token")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            if token != *upload {
                return error(403, 0);
            }
            let Some((rev, index, verifier)) = state.uploads.remove(*upload) else {
                return error(404, 0);
            };
            let data = multipart_block(&req.body);
            let code = state.revisions[&rev].code.clone();
            if verification_token(&code, &data) != verifier {
                return error(422, 2000);
            }
            let blob = state.id("blob");
            state.blobs.insert(blob.clone(), data.clone());
            let revision = state.revisions.get_mut(&rev).unwrap();
            revision.blocks.insert(index, blob);
            revision.sizes += data.len() as u64;
            ok(json!({}))
        }
        ("GET", ["storage", blob]) => match state.blobs.get(*blob) {
            Some(bytes) => ResponseTemplate::new(200).set_body_bytes(bytes.clone()),
            None => error(404, 0),
        },
        ("PUT", ["drive", "v2", "volumes", VOLUME, "files", link, "revisions", rev]) => {
            let Some(revision) = state
                .revisions
                .get(*rev)
                .filter(|r| r.link == *link && r.state == 0)
            else {
                return error(422, 2501);
            };
            if body["ManifestSignature"].as_str().is_none()
                || body["ChecksumVerified"] != json!(true)
            {
                return error(422, 2001);
            }
            let size = revision.sizes;
            for r in state
                .revisions
                .values_mut()
                .filter(|r| r.link == *link && r.state == 1)
            {
                r.state = 2;
            }
            state.revisions.get_mut(*rev).unwrap().state = 1;
            let file = state.links.get_mut(*link).unwrap();
            let was_draft = file.json["State"] == json!(0);
            file.json["State"] = json!(1);
            file.json["Size"] = json!(size);
            file.json["ModifyTime"] = json!(now_s());
            file.json["FileProperties"]["ActiveRevision"] = json!({ "ID": rev });
            let json = file.json.clone();
            // Neue Revision einer bestehenden Datei: Update-Event. Beim ersten Commit kein
            // eigenes Event (ungünstigster Fall für den Store).
            if !was_draft {
                state.event(2, json);
            }
            ok(json!({}))
        }
        ("GET", ["drive", "shares", SHARE, "files", _, "revisions", rev]) => {
            let Some(revision) = state.revisions.get(*rev) else {
                return error(422, 2501);
            };
            let from: u32 = query
                .get("FromBlockIndex")
                .map_or(1, |v| v.parse().unwrap());
            let size: usize = query.get("PageSize").map_or(50, |v| v.parse().unwrap());
            let blocks: Vec<Value> = revision
                .blocks
                .range(from..)
                .take(size)
                .map(|(index, blob)| json!({ "Index": index, "Token": blob, "BareURL": format!("{origin}/storage/{blob}"), "Hash": "" }))
                .collect();
            ok(json!({ "Revision": { "ID": rev, "State": revision.state, "Blocks": blocks } }))
        }
        ("POST", ["drive", "shares", SHARE, "folders", _, "trash_multiple"]) => {
            let mut responses = Vec::new();
            for id in body["LinkIDs"].as_array().unwrap() {
                let id = id.as_str().unwrap();
                let code = match state.links.get_mut(id) {
                    Some(link) => {
                        link.json["State"] = json!(2);
                        link.json["Trashed"] = json!(now_s());
                        // Live (P6-04): das Papierkorb-Event reicht nicht, um den Pfad zu bestimmen;
                        // hier nur Kennungen wie bei v2-Events. Erst das Lösch-Event zählt.
                        let json = json!({ "LinkID": id, "IsTrashed": true });
                        state.event(2, json);
                        1000
                    }
                    None => 2501,
                };
                responses.push(json!({ "LinkID": id, "Response": { "Code": code } }));
            }
            ResponseTemplate::new(200)
                .set_body_json(json!({ "Code": 1001, "Responses": responses }))
        }
        ("POST", ["drive", "v2", "volumes", VOLUME, "trash", "delete_multiple"]) => {
            purge(state, body, |l| l.json["State"] == json!(2), 2511)
        }
        ("POST", ["drive", "shares", SHARE, "folders", _, "delete_multiple"]) => {
            purge(state, body, |l| l.json["State"] == json!(0), 2501)
        }
        ("GET", ["drive", "volumes", VOLUME, "events", "latest"]) => {
            ok(json!({ "EventID": state.next.to_string() }))
        }
        ("GET", ["drive", "volumes", VOLUME, "events", id]) => {
            let Ok(since) = id.parse::<u64>() else {
                return ok(
                    json!({ "EventID": state.next.to_string(), "Events": [], "More": 0, "Refresh": 1 }),
                );
            };
            let newer: Vec<&(u64, Value)> =
                state.events.iter().filter(|(e, _)| *e > since).collect();
            let page: Vec<Value> = newer
                .iter()
                .take(EVENT_PAGE)
                .map(|(_, e)| e.clone())
                .collect();
            let last = newer[..newer.len().min(EVENT_PAGE)]
                .last()
                .map_or(since.max(state.next), |(e, _)| *e);
            let more = u8::from(newer.len() > EVENT_PAGE);
            ok(json!({ "EventID": last.to_string(), "Events": page, "More": more, "Refresh": 0 }))
        }
        _ => error(404, 0),
    }
}

/// Endgültig löschen, wenn `allowed`; sonst `refused`-Code je Link.
fn purge(
    state: &mut State,
    body: &Value,
    allowed: impl Fn(&FakeLink) -> bool,
    refused: i64,
) -> ResponseTemplate {
    let mut responses = Vec::new();
    for id in body["LinkIDs"].as_array().unwrap() {
        let id = id.as_str().unwrap();
        let code = match state.links.get(id) {
            Some(link) if allowed(link) => {
                state.links.remove(id);
                state.revisions.retain(|_, r| r.link != id);
                state.event(0, json!({ "LinkID": id }));
                1000
            }
            Some(_) => refused,
            None => 2501,
        };
        responses.push(json!({ "LinkID": id, "Response": { "Code": code } }));
    }
    ResponseTemplate::new(200).set_body_json(json!({ "Code": 1001, "Responses": responses }))
}
