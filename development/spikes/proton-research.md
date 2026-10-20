# P2-01 · Recherche: Wege zur Proton-API

Stand: 2026-10-17. Quellen: GitHub (Code + README), crates.io, npm, Proton-Blog/ToS. **Keine** Logins,
**keine** Requests gegen Proton-APIs. Markierung: **[V]** = an der Quelle geprüft, **[I]** = eigene Schlussfolgerung.
Zentrale Aussagen (SDK-README-Regeln, `external-drive`-Client-ID, Fork-Flow, Krypto-Migration) wurden in einer
zweiten Runde nochmals direkt an der Quelle verifiziert.

---

## 0. Kurzfassung

1. **Es gibt ein offizielles Proton-Drive-SDK** (MIT, TS + C#, sehr aktiv) – aber **ohne Login/Session**, und laut README
   „not yet ready for third-party production use". **Persönliche, nicht-kommerzielle Projekte sind ausdrücklich erlaubt**,
   unter Auflagen (ehrlicher `x-pm-appversion`, Events statt Polling, kein Proton-Branding, Drittanbieter-Hinweis beim Login).
2. **Login ohne Passwort-Handling ist möglich:** Proton nutzt für Drittanbieter-Forks seines CLI einen **Session-Fork über
   den Browser** (Client-ID `external-drive`). Unsere App sieht dabei weder Passwort noch 2FA noch CAPTCHA.
3. **Proton veröffentlicht Rust-Crates (MIT)** für SRP und Krypto (`proton-srp`, `proton-crypto`, `proton-crypto-account`,
   rein-Rust-Backend auf rpgp). Damit ist ein **Rust-Eigenbau** ohne Eigenbau-Krypto machbar.
4. **Größtes Risiko:** Proton kündigt ein **neues Krypto-Modell für Ende 2026 / Anfang 2027** an; Clients, die nur das alte
   implementieren, sind danach **nicht mehr interoperabel**. Das trifft jeden Weg außer „offizielles SDK nutzen".

**Empfehlung: Weg A – Rust-Eigenbau im `proton-adapter`**, Login per Browser-Session-Fork, Dateien < 128 KiB über den
„small file"-Endpunkt, Änderungen über Volume-Events. Begründung und Alternativen in §7.

---

## 1. Offizielles Proton Drive SDK

| Punkt | Befund |
|---|---|
| Repo | https://github.com/ProtonDriveApps/sdk |
| Lizenz | **MIT** – gilt nur für den Code; Nutzung des Dienstes unterliegt separaten Bedingungen [V] |
| Sprachen | TypeScript (`@protontech/drive-sdk`, npm 0.22.2, 2026-10-02), C# (v0.31.0, 2026-10-09). Kotlin/Swift „incubating" auf C#-NativeAOT-Kern mit C-ABI + Protobuf. **Kein Rust-Drive-Client** (nur ein Rust-Suchmodul im Inkubator) [V] |
| Aktivität | tägliche Commits, letzter Push 2026-10-09 [V] |
| Drittanbieter | „Personal, non-commercial projects: Allowed under Guidelines below." – „The SDK is not yet ready for third-party production use." [V] |
| Login | „The SDK provides functionality for Proton Drive business logic only" – Login, Session-Management, Adress-Provider **nicht enthalten** [V]. Inkubator-Modul `incubating/account` (JS + C#) implementiert SRP, 2FA, Refresh, Session-Fork – „nicht zur Veröffentlichung geplant, Übergangslösung bis zum Account-SDK" [V] |
| Krypto-Peer-Dep (TS) | `@protontech/crypto` ist **GPL-3.0** [V] → TS-SDK zieht GPL in die Distribution [I] |
| Krypto (C#) | `Proton.Cryptography` (github.com/ProtonDriveApps/dotnet-crypto, MIT) über gopenpgp/go-srp [V] |
| Offizielles CLI | `cli/` (Bun/TS, MIT); Login im Browser per Session-Fork. Quelltext: „Client ID for the authentication. cli-drive for the official CLI, external-drive for 3p forks." Basis-URL `drive-api.proton.me` [V] |
| Ankündigungen | Blog 2026-01-29 und 2026-06-04: Drittanbieter-Support „später", Linux-Client auf SDK-Basis in Arbeit [V]. **Kein** OAuth, kein Entwicklerprogramm, keine öffentliche API-Doku gefunden [V] |

### Betriebsregeln aus dem SDK-README (verbindlich für uns) [V, wörtlich geprüft]

- `x-pm-appversion` ehrlich setzen: `external-drive-{name}@{semver}-{channel}+{suffix}` – „Do not spoof or falsify this header."
  Für uns z. B. `external-drive-tasks@0.1.0-alpha` (kein „Proton" im Namen).
- „All HTTP requests must go to the official Proton Drive domain."
- „Do not poll the API or perform frequent recursive traversals of the file tree." – „Synchronize data using Drive events."
- „Your application must not use Proton logos, trademarks, or design elements."
- Beim Abfragen von Zugangsdaten: „This is a third-party application not officially supported by Proton."
- Caching, Event-Sync, Parallelitätslimits und exponentielles Backoff; Rate-Limits gelten pro Session und Nutzer;
  Verstöße → „limited or blocked". Einzelne Versionen können gesperrt werden.
- Neues Krypto-Modell „currently targeted for the end of 2026/early 2027 … may shift"; alte Clients „will not interoperate".

## 2. Proton-Bibliotheken

| Bibliothek | Lizenz | Sprache / aus Rust nutzbar | Aktivität | Bemerkung |
|---|---|---|---|---|
| ProtonMail/go-proton-api | MIT | Go – nur als Sidecar/cgo | 2026-10-09 | Core, Auth (inkl. FIDO2-, Two-Password-Typen), Drive-Endpunkte; **keine** Drive-Krypto |
| ProtonMail/go-srp | MIT | Go | 2025-01 | |
| ProtonMail/gopenpgp | MIT | Go | 2026-10 | |
| **ProtonMail/proton-crypto-rs** | **MIT** (crates.io zeigt „non-standard", da `license-file`) | **Rust**: `proton-srp` 0.8.4 (rein Rust, wasm32-cfg), `proton-crypto` 0.14.1, `proton-crypto-account` 0.20.1, `proton-rpgp` 0.9.0, `gopenpgp-sys` | 2026-10-08 | Default-Backend gopenpgp (Go zur Build-Zeit!); `--no-default-features --features rustpgp` = rein Rust. README: „not intended or vetted for general usage outside Proton", keine externen Beiträge |
| muon (Protons Rust-HTTP-Client) | **AGPL-3.0** | Rust, nicht auf crates.io | 2026-06 | **Meiden** (AGPL) |
| proton-pass-common | GPL-3.0 | Rust | aktiv | irrelevant, GPL |
| rpgp (`pgp` 0.21) | MIT/Apache-2.0 | Rust | 2026-10 | Basis des rustpgp-Backends; RFC 9580 [I] |

## 3. Community

| Projekt | Lizenz | Status | Bemerkung |
|---|---|---|---|
| rclone-Backend `protondrive` | MIT | Tier 4 „Experimental/Beta" (Doku 2026-07-31) | Passwort-Login + TOTP, keine Events, Forenberichte über 422/401 beim 2FA-Schritt (Juni 2026). Basis: Forks `rclone/Proton-API-Bridge`, `rclone/go-proton-api` (aktiv bis 2026-08) |
| henrybear327/Proton-API-Bridge | MIT | **archiviert** (2025-01) | |
| LeanderBB/proton-api-rs | **AGPL-3.0** | tot (2023) | |
| emersion/hydroxide | MIT | archiviert | |
| narrrl/proton-sdk-rs (`proton-sdk`, `proton-drive-rs` 0.9.2) | MIT | neu (2026-06), 1 Autor, 2 Sterne | Rein-Rust-Nachbau des Drive-SDK (rpgp, reqwest), SRP+TOTP, Events. Genutzt von narrrl/proton-drive-linux. Qualität unbekannt [I] |
| weitere | GPL-3.0/Apache-2.0 | | syncative, kio-protondrive, proton-drive-sync-engine – nur Namen/Lizenzen geprüft |

## 4. Nutzungsbedingungen / Drittanbieter

- ToS (Stand 2026-10-09): automatisierter Zugriff erlaubt, „provided that the resulting traffic remains indistinguishable
  from the standard client behavior of human users"; sonst Sperre möglich (mit Einspruchsweg) [V].
- Drittanbieter-Clients nicht ausdrücklich in der ToS geregelt; **operative Regel ist das SDK-README** (§1) [V].
- Keine öffentlich berichteten Sperren speziell für inoffizielle Drive-Clients gefunden [V].
- Offene Frage: Zählt eine offene, nicht-kommerzielle Todo-App als „personal project"? Für den Prototyp (Eigenbedarf): ja [I].

## 5. Auth-Ablauf

| Thema | Befund |
|---|---|
| Basis-URLs | `drive-api.proton.me` (SDK/CLI) [V]; `mail.proton.me/api` (go-proton-api) [V]; Login-Seite `account.proton.me` [V] |
| **Session-Fork (empfohlen)** | 1) ggf. `POST /auth/v4/sessions` mit `x-enforce-unauthsession: true` (unauth. Session) [V] · 2) `GET /auth/v4/sessions/forks` → `Selector`, `UserCode` [V] · 3) 32 Zufallsbytes erzeugen, Browser öffnen: `https://account.proton.me/desktop/login?app=drive&pv=3#payload=0:{UserCode}:{b64key}:external-drive` (Schlüssel nur im URL-Fragment, geht nicht an den Server) [V] · 4) alle 5 s (Startverzögerung 5 s, max. 10 min) `GET /auth/v4/sessions/forks/{selector}` → `UID`, `AccessToken`, `RefreshToken`, `Payload` [V] · 5) `Payload` = AES-256-GCM (12 B Nonce vorn, 16 B Tag hinten, AAD `"fork"`) → JSON `{keyPassword}` [V]. Welcher Status/Code „noch nicht fertig" bedeutet, ist im Code nicht sichtbar (Bericht: HTTP 422) → **in P2-02 beobachten** |
| SRP (Alternative) | `POST /core/v4/auth/info` (`Intent: "Proton"`, `Username`) → Version, Modulus, ServerEphemeral, Salt, SRPSession · `POST /core/v4/auth` (ClientEphemeral, ClientProof, SRPSession, PersistentCookies) → `ServerProof` prüfen [V]; `GET /core/v4/auth/modulus` [V] |
| 2FA | `POST /auth/v4/2fa` (TOTP); FIDO2-Typen in go-proton-api [V] |
| Refresh | `POST /auth/v4/refresh` `{ResponseType:"token", GrantType:"refresh_token", RefreshToken}`; bei HTTP 401 auslösen; Code 10013 = Refresh-Token ungültig [V] |
| Logout | `DELETE /auth/v4` [V] |
| Header | `x-pm-appversion`, `x-pm-uid`, `Authorization: Bearer <AccessToken>` [V] |
| Schlüssel-Passwort | `GET /core/v4/keys/salts` + bcrypt-basiertes `computeKeyPassword` (bei SRP nötig); **beim Fork direkt im Payload** [V]. Two-Password-Mode: separates Mailbox-Passwort [V] |
| Weitere | `/core/v4/users`, `/core/v4/addresses`, `/core/v4/keys/all`, `/core/v4/settings` [V] |
| Fehlercodes (go-proton-api) | 1000 OK, 1001 Multi, 2001 ungültiger Wert, 5001/5003 App-Version fehlt/ungültig, 8002 falsches Passwort, 9001 Human Verification nötig, 10013 Refresh ungültig, 12087 HV-Token ungültig [V] |
| Human Verification | 9001 liefert `HumanVerificationMethods` + `HumanVerificationToken`; Wiederholung mit `x-pm-human-verification-token[-type]` [V]. **Beim Fork-Login übernimmt das die Proton-Webseite** [I] |
| Rate-Limit | HTTP 429 + `Retry-After` (s, Default 10); SDK pausiert nach 50× 429 für 60 s [V] |

## 6. Drive-Datenmodell

**Hierarchie:** Volume → Share (Root-Share „my-files", `drive/v2/shares/my-files`) → Link/Node (Ordner/Datei) → Revision → Blöcke.

**Schlüsselkette** (SDK-Code [V], Adress-Schicht teils [I]):

| Schlüssel | entsperrt durch / Zweck |
|---|---|
| User-Key | mit `keyPassword` |
| Adress-Key | über User-Key; signiert Namen, Passphrasen, Blöcke, Manifest |
| Share-Key | Passphrase verschlüsselt an Adress-Key |
| Node-Key | Passphrase verschlüsselt an Eltern-Node-Key (Root: Share-Key); `NodePassphraseSignature` mit Adress-Key |
| Name | verschlüsselt mit Eltern-Node-Key, signiert |
| `NodeHashKey` (Ordner) | `NameHash = HMAC-SHA256(hashKey, name)` hex – Grundlage der Duplikat-Erkennung |
| Content-Session-Key (Datei) | `ContentKeyPacket` an Node-Key; `ContentKeyPacketSignature` |
| Blöcke | mit Session-Key verschlüsselt, Signatur mit Adress-Key als `EncSignature` |
| `ManifestSignature` | Adress-Key über verkettete Block-Hashes |
| `XAttr` | mit Node-Key verschlüsselt |

SDK aktiviert AEAD für Encryption-Keys → Krypto muss v6/SEIPDv2 können [V/I].

**Endpunkte** [V]: Volumes `drive/volumes` · Kinder `drive/v2/volumes/{v}/folders/{id}/children` · Batch `drive/v2/volumes/{v}/links` ·
Ordner/Datei anlegen `…/folders`, `…/files` · **kleine Dateien < 128 KiB in einem Multipart-Request:
`POST drive/v2/volumes/{v}/files/small`** und `…/files/{id}/revisions/small` (passt genau zu unseren kleinen,
unveränderlichen Segmenten) · große Dateien: `POST drive/blocks` → Storage-URLs (`pm-storage-token`), Commit
`PUT …/files/{id}/revisions/{rev}` · Papierkorb/Löschen `…/trash_multiple`, `…/delete_multiple` · Name prüfen
`…/links/{parent}/checkAvailableHashes` · **Events: `GET drive/volumes/{v}/events/latest`, `GET drive/v2/volumes/{v}/events/{eventId}`
(`More`-Flag)**; Share-Events veraltet.

**Fehler** [V]: **2500 ALREADY_EXISTS** (Namens-Hash-Kollision → unser `create` ohne Überschreiben = `CoreError::Conflict`),
2501 NOT_EXISTS, 2011 keine Berechtigung, 200001–200003 Quota, 200300 zu viele Kinder.

## 7. CORS (Web-Sync)

Preflight-Test (`OPTIONS` mit fremder Origin) **noch nicht durchgeführt**: In dieser Sitzung blockiert (Netzwerk-Proben
gegen Proton-Hosts werden vom Sitzungs-Sicherheitsfilter abgelehnt); der User kann ihn selbst ausführen (Befehl in der
Session-Übergabe). Erwartung [I]: keine CORS-Freigabe für fremde Origins (Proton-Webclients laufen same-origin).
Folge, falls bestätigt: Web-Sync nur über Same-Origin-Proxy oder Web ohne Sync – Entscheidung in P2-06 bzw. vertagt
(Web ist nicht im Prototyp).

## 8. Wege & Empfehlung

| | **A – Rust-Eigenbau** | B – Offizielles SDK als Sidecar/FFI | C – CLI/rclone als Sidecar |
|---|---|---|---|
| Kern | `proton-srp`, `proton-crypto(-account)` mit `rustpgp` (MIT) bzw. rpgp; reqwest; eigener Mini-Drive-Client nur für **einen** App-Ordner | C#-SDK über C-ABI (NativeAOT) oder TS-SDK in Node/Bun | offizielles Drive-CLI (MIT) bzw. rclone |
| Login | Browser-Session-Fork (`external-drive`) | Fork aus Inkubator-Modul (unveröffentlicht) | CLI: Browser-Fork; rclone: Passwort+TOTP |
| Lizenz | MIT-Deps → passt | C#: MIT; TS: GPL-3.0-Krypto | MIT |
| Performance/RAM | ✓ (ein Prozess) | ✗ .NET-Runtime zig MB bzw. Node-Prozess – sprengt RAM-Budget | ✗ Prozess pro Aufruf, Bun-Binary |
| Web-Pfad | ✓ (rein Rust → wasm denkbar) | ✗ | ✗ |
| Krypto-Migration | ✗ Nachziehen nötig – hoffentlich über Proton-Crates [I] | ✓ macht Proton | ✓ (CLI) |
| Aufwand | mittel (schmaler Umfang: kleine Dateien, keine Blöcke, Events) | mittel–hoch (FFI/IPC, Pre-1.0-Churn) | gering für PoC, hoch für sauberen Sync |

**Empfehlung: A.** Passt zu Architektur (hexagonal, alles in `proton-adapter`), Performance-Budgets, Lizenz und Web-Ziel.
Umfang bewusst schmal halten: nur App-Ordner, nur `files/small`, Events statt Listing-Polling.
**C (offizielles CLI) als Notnagel** für den Fall, dass A an der Krypto scheitert. Das MIT-SDK dient als
**Referenz zum Verstehen** – kein Code kopieren (AGENTS.md).

**Login-Empfehlung:** **Session-Fork über den Browser** statt SRP: kein Passwort in unserer App, 2FA/FIDO2/CAPTCHA/
Two-Password-Mode erledigt Proton, entspricht dem Vorgehen des offiziellen CLI für Drittanbieter-Forks.
Folge für `AuthProvider`: statt Passwort-State-Machine ein Schritt „Öffne URL im Browser → warte" (P2-06 anpassen).
SRP bleibt als Plan B (P2-02/03 würden ihn nur dokumentieren statt implementieren – Entscheidung des Users).

**Folgen für bestehende Planung:**
- `sync-strategy.md`: Remote-Änderungen über Drive-Events erkennen, **kein periodisches Listing** (README-Regel).
- `x-pm-appversion` = `external-drive-<produktname>@<semver>-<channel>`; Login-UI mit Drittanbieter-Hinweis.
- Krypto-Migration Ende 2026/Anfang 2027: vor P6 erneut prüfen; Adapter so bauen, dass die Krypto-Schicht austauschbar ist.
- ADR-Nummer: ADR-010…012 sind bereits vergeben (P1) → Spike-Abschluss wird **ADR-013**.
- Spike-Code: `spikes/` wird **gitignored** (Wegwerf-Code, Debug-Ausgaben sollen nie versehentlich ins Repo).

## 9. Offene Fragen

1. Status/Code für „Fork noch nicht bestätigt" beim Polling (Bericht: 422) – in P2-02 beobachten.
2. Ist `external-drive` für Nicht-CLI-Drittanbieter-Apps gedacht/stabil? (Quelltext sagt „3p forks")
3. Wann genau und wie kommt das neue Krypto-Modell; ziehen `proton-crypto-rs`/rpgp mit?
4. Wird das Account-SDK veröffentlicht (dann Login darüber)?
5. CORS für fremde Origins (Test ausstehend).
6. Konkrete Rate-Limit-Werte; empfohlenes Event-Abfrageintervall.
7. Reicht `proton-crypto-account` (rustpgp) für die komplette Schlüsselkette inkl. AEAD/v6, oder fehlt etwas?
8. Adress-Key-Schicht im Detail (nicht im Quelltext verifiziert).

## 10. Quellen

- https://github.com/ProtonDriveApps/sdk (README, `cli/src/config.ts`, `incubating/account/js/src/{authWeb,accountApi}.ts`)
- https://proton.me/blog/drive-sdk-january-2026 · https://proton.me/blog/drive-sdk-june-2026
- https://github.com/ProtonMail/proton-crypto-rs · https://crates.io/crates/proton-srp
- https://github.com/ProtonMail/go-proton-api
- https://rclone.org/protondrive/ · https://github.com/rclone/Proton-API-Bridge
- https://github.com/narrrl/proton-sdk-rs
- https://proton.me/legal/terms
- https://github.com/ProtonDriveApps/dotnet-crypto

## 11. Entscheidung des Users (2026-10-17)

- **Weg A** (Rust-Eigenbau) ist Arbeitsgrundlage für P2-02…P2-05; endgültig in P2-06 (ADR-013).
- **Login:** Browser-Session-Fork. P2-02 = Fork-Login; P2-03 testet Refresh, Logout und Fehlercodes real,
  SRP/2FA/Human Verification werden nur dokumentiert (übernimmt Proton im Browser). Kein TOTP-Setup nötig.
- Proben mit Zugangsdaten startet der User selbst im eigenen Terminal; Claude sieht keine Zugangsdaten.

## 12. Erkenntnisse aus dem Probe-Bau (offline, 2026-10-17)

Probe: `spikes/proton-probe/` (gitignored), Befehle `login`, `session`, `drive-read [Datei]`, `drive-write`.
Live-Ergebnisse stehen noch aus (User führt aus) → werden in §13 ergänzt.

- **Krypto vollständig mit Proton-Rust-Crates abbildbar** (`proton-crypto` 0.14.1 + `proton-crypto-account` 0.20.1,
  Feature `rustpgp`, kein Go): `UserKeys::unlock(provider, &KeySecret(keyPassword))` →
  `AddressKeys::unlock(provider, user_keys, Some(secret))` (inkl. Token-basierter Adress-Keys) → Share-/Node-Keys per
  `private_key_import`, Namen/Passphrasen per Decryptor, Signaturen per Verifier, Session-Keys per `decrypt_session_key`.
- **Format für neue Inhalte** (laut rclone-Fork `rclone/go-proton-api`, Kommentare von 2026): Die offiziellen Apps lesen
  Crypto-Refresh-Dateien (v6-Node-Keys, SEIPD v2) noch nicht zuverlässig → neue Nodes **v4**, Content-Key **PKESK v3**,
  Blöcke **SEIPD v1**; Namen, Passphrasen, XAttr, Block-Signaturen **nie AEAD**. Das Default-Profil von `proton-rpgp`
  erzeugt genau das (AEAD nur mit `HAZARD_AEAD_PROFILE`) – per Test mit Paket-Header-Prüfung abgesichert.
- **Schreibpfad (v1-Share-Endpunkte, wie rclone):** `POST /drive/shares/{s}/folders` (ParentLinkID, Name, Hash, NodeKey,
  NodeHashKey, NodePassphrase(+Signature), SignatureAddress) · `POST /drive/shares/{s}/files` → `{File:{ID,RevisionID}}`
  (+ MIMEType, ContentKeyPacket (base64), ContentKeyPacketSignature) · `POST /drive/blocks` → `UploadLinks[{Token,BareURL}]`
  · Multipart-Feld `Block` an `BareURL` mit `pm-storage-token` · `PUT /drive/shares/{s}/files/{l}/revisions/{r}`
  (ManifestSignature = Adress-Signatur über verkettete SHA-256 der verschlüsselten Blöcke, SignatureAddress, XAttr).
  Duplikat: HTTP 422 + Code 2500; offener Entwurf: HTTP 409 (ADraftExist).
- **Wer signiert was:** Name/Passphrase/Manifest/Block-Klartext → Adress-Key; ContentKeyPacketSignature und
  NodeHashKey → eigener Node-Key; Block-Signatur wird zusätzlich an den Node-Key verschlüsselt (`EncSignature`).
- Name-Hash = hex(HMAC-SHA256(NodeHashKey des Elternordners, Klarname)) – per RFC-4231-Testvektor geprüft.
- Offline-Roundtrip-Test (Schreib- → Lesepfad, alle Schlüssel lokal erzeugt) grün.
- Lesepfad nutzt v1-Endpunkte; die v2-Endpunkte des SDK (`files/small`, `v2/volumes/...`) sind noch nicht probiert.

## 13. Live-Ergebnisse

- **P2-02 (2026-10-17):** Fork-Login gegen das Testkonto laut User **problemlos** (`proton-probe login`: Browser-Fork,
  Kontrollaufruf, Logout). Detailausgabe (Pending-Code, Feldnamen, Scopes) wurde nicht übermittelt – bei Bedarf
  in P6-03 nachholen.
- **P2-03 (`session`, 2026-10-17, Ausgabe gesehen):**
  - Fork: `GET /auth/v4/sessions/forks` 200/1000 → `Selector`, `UserCode`. Bestätigung im Browser nach ~5 s (User war
    bereits angemeldet) → kein Pending-Zustand beobachtet (422 laut SDK-Code, live nicht gesehen).
  - Fork-Antwort-Felder: AccessToken, ExpiresIn, LocalID, Payload, RefreshToken, Scope, Scopes, TokenType, UID, UserID.
    Scopes u. a. `full, user, loggedin, drive, docs, verified, settings`. Payload-Felder: `keyPassword`, `type`.
  - Fehlercodes: ohne `x-pm-appversion` → **400/5002** „Missing x-pm-appversion header"; ungültiges Format →
    **400/2064** „Invalid section name"; ungültiger/fehlender Token → **401/Code 401** „Invalid access token".
  - Refresh `POST /auth/v4/refresh` 200: **Refresh-Token rotiert**, UID bleibt, **alter Access-Token sofort ungültig**
    (401). Antwort zusätzlich mit `RefreshCounter`.
  - Logout `DELETE /auth/v4` 200 → alter Token 401; Refresh danach **422/10013** „Invalid refresh token".
  - Latenzen: 40–600 ms je Aufruf (unauth. Fehler ~50 ms, authentifiziert ~150–550 ms).
- **P2-04 (`drive-read`):** laut User problemlos (Ausgabe nicht übermittelt). Unabhängig belegt durch den
  `drive-write`-Lauf: User-Key (1/1) und Adress-Key (1/1, v4) entsperrt, Share-Key entsperrt, Root-Name „root"
  entschlüsselt, NodeHashKey entschlüsselt (**44 Bytes = Base64-Text**, nicht 32 Rohbytes).
  Share-Antwort enthält neue Felder (`PossibleKeyPackets`, `RootLinkRecoveryPassphrase`, `Memberships`, `VolumeType` …).
- **P2-05 (`drive-write`, 1. Lauf):** Event-Cursor (`events/latest` → `EventID`), Ordner anlegen (v1-Share-Endpunkt) und
  Datei-Entwurf anlegen funktionieren (`File`: `ID`, `RevisionID`, neu `ClientUID`). **Block-Upload abgelehnt:**
  `POST /drive/blocks` → **400/2000** „You are using an outdated version of the app. Please update to upload this file."
  Ursache laut aktuellem offiziellem SDK (C#): **Block-Verifikation ist Pflicht** –
  `GET /drive/v2/volumes/{v}/links/{l}/revisions/{r}/verification` → `VerificationCode` (+ `ContentKeyPacket`);
  je Block `Verifier: { Token = VerificationCode XOR erste Bytes des verschlüsselten Datenpakets }`; Block-Request mit
  `VolumeID` + `ThumbnailList`; Commit über `PUT /drive/v2/volumes/{v}/files/{l}/revisions/{r}` mit `ChecksumVerified`.
  rclone (Stand 2026-08) kennt das noch nicht → vermutlich ebenfalls betroffen. Probe angepasst.
- **P2-05 (`drive-write`, 2. Lauf, Ausgabe gesehen) – alle Operationen funktionieren:**
  - Ordner `TasksProbe/` wiederverwendet (Listing + Namen entschlüsseln: **242 ms**).
  - Upload 1 KB: `POST /drive/shares/{s}/files` (478 ms) → `GET …/v2/…/revisions/{r}/verification` (191 ms; Felder
    `VerificationCode` 32 B, `ContentKeyPacket`) → `POST /drive/blocks` mit `Verifier` (285 ms; Antwort zusätzlich
    `ThumbnailLink(s)`) → Storage-POST (366 ms) → `PUT /drive/v2/volumes/{v}/files/{l}/revisions/{r}` (353 ms).
    **Gesamt 1 679 ms** inkl. Krypto.
  - Download 1 KB: Link + Revision + Block (1 077 B verschlüsselt): **937 ms**; „Inhalt identisch: true".
    Revision-Felder u. a. `ChecksumVerified`, `ClientUID`, `Sdk`, `ThirdParty` (Server unterscheidet Drittclients).
  - **Duplikat:** `POST …/files` mit gleichem Namen → **HTTP 422 / Code 2500** („Datei oder Ordner mit diesem Namen
    existiert bereits." – Fehlertext lokalisiert, also nur den Code auswerten).
  - **Events:** `GET /drive/volumes/{v}/events/{id}` → `EventID`, `Events`, `More`, `Refresh`; 1 Event (EventType 1 =
    Create) mit vollständigem `Link`. v2-Variante `/drive/v2/volumes/{v}/events/{id}` liefert nur `LinkID`,
    `ParentLinkID`, `IsShared`, `IsTrashed` (+ `DisableSdkQuotaChecks` …) → Details müssen nachgeladen werden.
    Für `changes_since`: v1-Events genügen (Link inkl. verschlüsseltem Namen), `Refresh`-Flag = Vollabgleich nötig.
  - **Löschen:** `…/folders/{parent}/trash_multiple` → 200/1001 (Papierkorb). `…/folders/{parent}/delete_multiple`
    → je Link **2501 „Draft file not found"** – gilt nur für Entwürfe. Endgültiges Löschen braucht den Papierkorb-
    Endpunkt (SDK: `v2/volumes/{v}/trash/delete_multiple`, ungetestet). Gesamt 533 ms. Testdatei liegt im Papierkorb.
  - In der Weboberfläche laut User sichtbar/erkennbar; `.bin` (`application/octet-stream`) hat keine Vorschau – erwartbar.
  - Ein abgebrochener Entwurf aus Lauf 1 kann im Ordner liegen (Drafts verfallen serverseitig, sonst manuell löschen).
- **Schlussfolgerungen für den Adapter (P6):** pro Segment-Upload ~5 Requests ≈ 1,5–2 s → Push gebündelt und
  selten (Debounce/Batching wie geplant); Pull über v1-Events statt Listing; Löschen = Papierkorb + Papierkorb-Delete;
  `Conflict` an Code 2500 festmachen; Block-Verifikation Pflicht.
