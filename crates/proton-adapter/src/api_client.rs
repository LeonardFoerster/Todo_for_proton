//! HTTP-Client für die Proton-API: gemeinsame Header, Timeouts, Fehlerklassifizierung, Logging.
//!
//! Logging (`tracing`, Target `proton_adapter::http`): je Aufruf Methode, **Routen-Vorlage**,
//! Status, Proton-Code und Dauer. Nie konkrete Pfade (der Fork-Selector im Pfad ist ein
//! Geheimnis), Header, Bodies oder Server-Fehlertexte.

use std::time::{Duration, Instant};

use reqwest::{header::HeaderMap, multipart, Method};
use serde::de::DeserializeOwned;
use serde_json::Value;
use todo_core::{CoreError, CoreResult};

use crate::{
    error::{code, parse_retry_after, ApiError},
    secret::Secret,
};

/// API-Host des offiziellen Drive-SDK (Proton-Regel: nur die offizielle Drive-Domain).
pub const DEFAULT_BASE_URL: &str = "https://drive-api.proton.me";

const ACCEPT: &str = "application/vnd.protonmail.v1+json";
/// Höchstgröße einer API-Antwort (JSON). Großzügig für Ordnerseiten mit vielen Schlüsseln.
const MAX_API_BODY: usize = 16 * 1024 * 1024;
/// Höchstgröße eines Storage-Blocks (4 MiB Klartext plus Verschlüsselungs-Overhead).
pub(crate) const MAX_STORAGE_BODY: usize = 5 * 1024 * 1024;
/// Erlaubte Storage-Hosts: Proton-Domain (Unterdomains wie `fra-storage.proton.me`).
const STORAGE_DOMAIN: &str = "proton.me";
const LOG_TARGET: &str = "proton_adapter::http";

/// Verbindungsparameter. `app_version` muss ehrlich sein:
/// `external-drive-<name>@<semver>-<channel>` (Proton-SDK-Regeln, ADR-013).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiConfig {
    pub base_url: String,
    pub app_version: String,
    /// Gesamtdauer eines Aufrufs inkl. Body.
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl ApiConfig {
    #[must_use]
    pub fn new(app_version: impl Into<String>) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            app_version: app_version.into(),
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
        }
    }
}

/// Session-Header eines authentifizierten Aufrufs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Auth<'a> {
    pub(crate) uid: &'a str,
    pub(crate) access_token: &'a Secret,
}

/// Ein API-Aufruf. `route` ist die Vorlage fürs Log (z. B. `/drive/shares/{share}/files`).
#[derive(Debug, Clone)]
pub(crate) struct Request<'a> {
    method: Method,
    route: &'static str,
    path: String,
    json: Option<Value>,
    auth: Option<Auth<'a>>,
    /// Session-ID ohne Bearer-Token (Refresh, Fork-Polling einer unauth. Session).
    uid_only: Option<&'a str>,
}

impl<'a> Request<'a> {
    pub(crate) fn new(method: Method, route: &'static str, path: impl Into<String>) -> Self {
        Self {
            method,
            route,
            path: path.into(),
            json: None,
            auth: None,
            uid_only: None,
        }
    }

    pub(crate) fn get(route: &'static str, path: impl Into<String>) -> Self {
        Self::new(Method::GET, route, path)
    }

    pub(crate) fn post(route: &'static str, path: impl Into<String>) -> Self {
        Self::new(Method::POST, route, path)
    }

    /// Methode und Routen-Vorlage (für Fehlermeldungen; ohne Geheimnisse).
    pub(crate) fn describe(&self) -> String {
        format!("{} {}", self.method, self.route)
    }

    #[must_use]
    pub(crate) fn json(mut self, body: Value) -> Self {
        self.json = Some(body);
        self
    }

    #[must_use]
    pub(crate) fn auth(mut self, auth: Auth<'a>) -> Self {
        self.auth = Some(auth);
        self
    }

    #[must_use]
    pub(crate) fn uid(mut self, uid: &'a str) -> Self {
        self.uid_only = Some(uid);
        self
    }
}

/// Dünne Hülle um `reqwest::Client`; günstig zu klonen.
#[derive(Debug, Clone)]
pub(crate) struct ApiClient {
    http: reqwest::Client,
    config: ApiConfig,
}

impl ApiClient {
    /// # Errors
    /// `CoreError::Internal`, wenn der HTTP-Client nicht gebaut werden kann.
    pub(crate) fn new(config: ApiConfig) -> CoreResult<Self> {
        // rustls ohne aws-lc (C-Toolchain, Android): `ring` als Prozess-Standard, sofern die
        // Shell nicht schon einen anderen Provider installiert hat.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "tasks-proton-adapter/",
                env!("CARGO_PKG_VERSION"),
                " (third-party)"
            ))
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            // Keine Weiterleitungen: Sie trügen `x-pm-uid` bzw. `pm-storage-token` zu einem
            // anderen Ziel (reqwest entfernt nur Standard-Auth-Header).
            .redirect(reqwest::redirect::Policy::none())
            .https_only(config.base_url.starts_with("https://"))
            .build()
            .map_err(|e| CoreError::Internal(format!("http client: {}", e.without_url())))?;
        Ok(Self { http, config })
    }

    /// Führt einen Aufruf aus. Erfolg = 2xx und Proton-Code 1000/1001 (oder ohne Code).
    pub(crate) async fn send(&self, req: Request<'_>) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.config.base_url, req.path);
        let mut builder = self
            .http
            .request(req.method.clone(), url)
            .header("Accept", ACCEPT)
            .header("x-pm-appversion", &self.config.app_version);
        if let Some(auth) = req.auth {
            builder = builder
                .header("x-pm-uid", auth.uid)
                .bearer_auth(auth.access_token.expose());
        } else if let Some(uid) = req.uid_only {
            builder = builder.header("x-pm-uid", uid);
        }
        if let Some(body) = &req.json {
            builder = builder.json(body);
        }

        let started = Instant::now();
        let result = builder.send().await;
        let elapsed_ms = elapsed_ms(started);
        let response = match result {
            Ok(response) => response,
            Err(err) => {
                let err = transport(&err);
                tracing::warn!(target: LOG_TARGET, method = %req.method, route = req.route,
                    elapsed_ms, error = %err, "request failed");
                return Err(err);
            }
        };
        let status = response.status();
        let retry_after_s = retry_after(response.headers());
        let bytes = read_capped(response, MAX_API_BODY).await?;
        let body: Option<Value> = serde_json::from_slice(&bytes).ok();
        let proton_code = body
            .as_ref()
            .and_then(|b| b.get("Code"))
            .and_then(Value::as_i64);
        tracing::debug!(target: LOG_TARGET, method = %req.method, route = req.route,
            status = status.as_u16(), code = proton_code, elapsed_ms, "request");

        let ok_code = proton_code.is_none_or(|c| c == code::OK || c == code::MULTI);
        match body {
            Some(body) if status.is_success() && ok_code => Ok(body),
            None if status.is_success() => Err(ApiError::Decode(format!(
                "{} {}: body is not JSON",
                req.method, req.route
            ))),
            body => Err(ApiError::Status {
                status: status.as_u16(),
                code: proton_code,
                retry_after_s,
                details: body
                    .as_ref()
                    .and_then(|b| b.get("Details"))
                    .filter(|d| !d.is_null())
                    .map(Value::to_string),
            }),
        }
    }

    /// Wie [`Self::send`], deserialisiert die Antwort in `T`.
    pub(crate) async fn send_as<T: DeserializeOwned>(
        &self,
        req: Request<'_>,
    ) -> Result<T, ApiError> {
        let what = format!("{} {}", req.method, req.route);
        let body = self.send(req).await?;
        serde_json::from_value(body).map_err(|e| ApiError::Decode(decode_error(&what, &e)))
    }

    /// Lädt einen verschlüsselten Block zu einer Storage-URL hoch (Multipart-Feld `Block`).
    pub(crate) async fn upload_block(
        &self,
        url: &str,
        token: &Secret,
        bytes: Vec<u8>,
    ) -> Result<(), ApiError> {
        self.check_storage_url(url)?;
        let part = multipart::Part::bytes(bytes)
            .file_name("blob")
            .mime_str("application/octet-stream")
            .map_err(|e| ApiError::Decode(e.without_url().to_string()))?;
        let request = self
            .http
            .post(url)
            .header("pm-storage-token", token.expose())
            .header("x-pm-appversion", &self.config.app_version)
            .multipart(multipart::Form::new().part("Block", part));
        self.storage(Method::POST, request).await.map(drop)
    }

    /// Lädt einen verschlüsselten Block von einer Storage-URL.
    pub(crate) async fn download_block(
        &self,
        url: &str,
        token: &Secret,
    ) -> Result<Vec<u8>, ApiError> {
        self.check_storage_url(url)?;
        let request = self
            .http
            .get(url)
            .header("pm-storage-token", token.expose())
            .header("x-pm-appversion", &self.config.app_version);
        self.storage(Method::GET, request).await
    }

    async fn storage(
        &self,
        method: Method,
        request: reqwest::RequestBuilder,
    ) -> Result<Vec<u8>, ApiError> {
        let started = Instant::now();
        let result = request.send().await;
        let elapsed_ms = elapsed_ms(started);
        let response = result.map_err(|e| {
            let err = transport(&e);
            tracing::warn!(target: LOG_TARGET, %method, route = "<storage>", elapsed_ms,
                error = %err, "request failed");
            err
        })?;
        let status = response.status();
        let retry_after_s = retry_after(response.headers());
        let bytes = read_capped(response, MAX_STORAGE_BODY).await?;
        tracing::debug!(target: LOG_TARGET, %method, route = "<storage>",
            status = status.as_u16(), bytes = bytes.len(), elapsed_ms, "request");
        if status.is_success() {
            Ok(bytes)
        } else {
            Err(ApiError::Status {
                status: status.as_u16(),
                code: storage_code(&bytes),
                retry_after_s,
                details: None,
            })
        }
    }
}

impl ApiClient {
    /// Storage-URLs kommen vom Server; das Token geht nur per HTTPS an Proton-Hosts (bzw. bei
    /// einer abweichenden Basis-URL – Tests – an deren Origin).
    fn check_storage_url(&self, url: &str) -> Result<(), ApiError> {
        let rejected = || ApiError::Decode("storage url is not allowed".into());
        let url = reqwest::Url::parse(url).map_err(|_| rejected())?;
        let base = reqwest::Url::parse(&self.config.base_url).map_err(|_| rejected())?;
        let host = url.host_str().unwrap_or_default();
        let proton = url.scheme() == "https"
            && (host == STORAGE_DOMAIN || host.ends_with(&format!(".{STORAGE_DOMAIN}")));
        let same_origin = url.origin() == base.origin();
        if proton || (same_origin && self.config.base_url != DEFAULT_BASE_URL) {
            Ok(())
        } else {
            Err(rejected())
        }
    }
}

/// Liest den Body höchstens bis `max` Bytes (sonst Fehler statt unbegrenzt Speicher).
async fn read_capped(mut response: reqwest::Response, max: usize) -> Result<Vec<u8>, ApiError> {
    let too_large = || ApiError::Decode("response body too large".into());
    if response
        .content_length()
        .is_some_and(|len| len > max as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| transport(&e))? {
        if bytes.len() + chunk.len() > max {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Decode-Fehler ohne den Text von serde (der Werte aus der Antwort zitieren kann).
pub(crate) fn decode_error(what: &str, err: &serde_json::Error) -> String {
    format!(
        "{what}: unexpected response ({:?} at {}:{})",
        err.classify(),
        err.line(),
        err.column()
    )
}

fn storage_code(body: &[u8]) -> Option<i64> {
    serde_json::from_slice::<Value>(body)
        .ok()?
        .get("Code")?
        .as_i64()
}

/// Transportfehler ohne URL (enthielte z. B. den Fork-Selector).
fn transport(err: &reqwest::Error) -> ApiError {
    let kind = if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_body() || err.is_decode() {
        "body"
    } else {
        "request"
    };
    ApiError::Transport(kind.to_owned())
}

fn retry_after(headers: &HeaderMap) -> Option<u32> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(parse_retry_after)
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{cell::RefCell, io, sync::Once};

    use serde_json::json;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use super::*;

    pub(crate) const APP_VERSION: &str = "external-drive-tasks@0.1.0-test";

    pub(crate) fn client(server: &MockServer) -> ApiClient {
        let mut config = ApiConfig::new(APP_VERSION);
        config.base_url = server.uri();
        config.timeout = Duration::from_secs(2);
        ApiClient::new(config).expect("client")
    }

    /// Bereinigte Antworten aus den P2-Proben (Feldnamen echt, Werte erfunden).
    pub(crate) fn fixture(name: &str) -> Value {
        let text = match name {
            "missing_app_version" => include_str!("../tests/fixtures/missing_app_version.json"),
            "invalid_app_version" => include_str!("../tests/fixtures/invalid_app_version.json"),
            "invalid_access_token" => include_str!("../tests/fixtures/invalid_access_token.json"),
            "invalid_refresh_token" => {
                include_str!("../tests/fixtures/invalid_refresh_token.json")
            }
            "already_exists" => include_str!("../tests/fixtures/already_exists.json"),
            "outdated_app" => include_str!("../tests/fixtures/outdated_app.json"),
            "users" => include_str!("../tests/fixtures/users.json"),
            other => panic!("unknown fixture {other}"),
        };
        serde_json::from_str(text).expect("fixture json")
    }

    #[tokio::test]
    async fn sends_common_and_session_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/core/v4/users"))
            .and(header("x-pm-appversion", APP_VERSION))
            .and(header("accept", ACCEPT))
            .and(header("x-pm-uid", "uid-1"))
            .and(header("authorization", "Bearer tok-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture("users")))
            .expect(1)
            .mount(&server)
            .await;
        let token = Secret::new("tok-1");
        let body = client(&server)
            .send(Request::get("/core/v4/users", "/core/v4/users").auth(Auth {
                uid: "uid-1",
                access_token: &token,
            }))
            .await
            .expect("ok");
        assert_eq!(body["User"]["Name"], "testuser");
    }

    #[test]
    fn storage_urls_must_be_proton_https() {
        let api = ApiClient::new(ApiConfig::new(APP_VERSION)).expect("client");
        assert!(api
            .check_storage_url("https://fra-storage.proton.me/storage/blocks/x")
            .is_ok());
        for bad in [
            "http://fra-storage.proton.me/storage/blocks/x",
            "https://proton.me.evil.example/x",
            "https://evil.example/x",
            "http://127.0.0.1:9/x",
            "not a url",
        ] {
            assert!(api.check_storage_url(bad).is_err(), "{bad}");
        }
    }

    /// Eine Weiterleitung wird nicht verfolgt (sie trüge Session- bzw. Storage-Header woandershin).
    #[tokio::test]
    async fn redirects_are_not_followed() {
        let server = MockServer::start().await;
        Mock::given(path("/start"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/elsewhere"))
            .mount(&server)
            .await;
        Mock::given(path("/elsewhere"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "Code": 1000 })))
            .expect(0)
            .mount(&server)
            .await;
        let err = client(&server)
            .send(Request::get("/start", "/start"))
            .await
            .unwrap_err();
        assert_eq!(err.status(), Some(302));
    }

    #[tokio::test]
    async fn oversized_bodies_are_rejected() {
        let server = MockServer::start().await;
        Mock::given(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b' '; MAX_API_BODY + 1]))
            .mount(&server)
            .await;
        let err = client(&server)
            .send(Request::get("/big", "/big"))
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Decode(_)), "{err:?}");
    }

    #[tokio::test]
    async fn proton_error_bodies_are_classified() {
        let server = MockServer::start().await;
        for (route, status, fixture_name) in [
            ("/a", 400, "missing_app_version"),
            ("/b", 400, "invalid_app_version"),
            ("/c", 401, "invalid_access_token"),
            ("/d", 422, "invalid_refresh_token"),
            ("/e", 422, "already_exists"),
            ("/f", 400, "outdated_app"),
        ] {
            Mock::given(path(route))
                .respond_with(ResponseTemplate::new(status).set_body_json(fixture(fixture_name)))
                .mount(&server)
                .await;
        }
        let api = client(&server);
        let err = |p: &'static str| {
            let api = api.clone();
            async move { api.send(Request::get("/x", p)).await.unwrap_err() }
        };
        assert_eq!(err("/a").await.code(), Some(code::APP_VERSION_MISSING));
        assert_eq!(
            err("/b").await.code(),
            Some(code::INVALID_APP_VERSION_FORMAT)
        );
        assert!(err("/c").await.is_unauthorized());
        assert_eq!(err("/d").await.code(), Some(code::INVALID_REFRESH_TOKEN));
        assert!(matches!(
            CoreError::from(err("/e").await),
            CoreError::Conflict(_)
        ));
        assert_eq!(err("/f").await.code(), Some(code::OUTDATED_APP));
    }

    #[tokio::test]
    async fn non_1000_code_in_success_status_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(path("/x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "Code": 2001 })))
            .mount(&server)
            .await;
        let err = client(&server)
            .send(Request::get("/x", "/x"))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Some(2001));
    }

    #[tokio::test]
    async fn rate_limit_reads_retry_after() {
        let server = MockServer::start().await;
        Mock::given(path("/x"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "17")
                    .set_body_json(json!({ "Code": 2028, "Error": "Too many requests" })),
            )
            .mount(&server)
            .await;
        let err = client(&server)
            .send(Request::get("/x", "/x"))
            .await
            .unwrap_err();
        assert_eq!(
            CoreError::from(err),
            CoreError::RateLimited { retry_after_s: 17 }
        );
    }

    #[tokio::test]
    async fn server_errors_and_html_bodies_are_retryable() {
        let server = MockServer::start().await;
        Mock::given(path("/x"))
            .respond_with(ResponseTemplate::new(502).set_body_string("<html>bad gateway</html>"))
            .mount(&server)
            .await;
        let err = client(&server)
            .send(Request::get("/x", "/x"))
            .await
            .unwrap_err();
        assert!(CoreError::from(err).is_retryable());
    }

    #[tokio::test]
    async fn timeout_is_network_error() {
        let server = MockServer::start().await;
        Mock::given(path("/slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "Code": 1000 }))
                    .set_delay(Duration::from_secs(5)),
            )
            .mount(&server)
            .await;
        let mut config = ApiConfig::new(APP_VERSION);
        config.base_url = server.uri();
        config.timeout = Duration::from_millis(200);
        let err = ApiClient::new(config)
            .unwrap()
            .send(Request::get("/slow", "/slow"))
            .await
            .unwrap_err();
        assert_eq!(err, ApiError::Transport("timeout".into()));
        assert!(matches!(CoreError::from(err), CoreError::Network(_)));
    }

    #[tokio::test]
    async fn unreachable_host_is_network_error() {
        let mut config = ApiConfig::new(APP_VERSION);
        // Port 9 (discard) auf localhost: Verbindung wird abgelehnt.
        config.base_url = "http://127.0.0.1:9".into();
        let err = ApiClient::new(config)
            .unwrap()
            .send(Request::get("/x", "/x"))
            .await
            .unwrap_err();
        assert!(CoreError::from(err).is_retryable());
    }

    #[tokio::test]
    async fn storage_roundtrip_sends_token_and_multipart_field() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/storage/blocks"))
            .and(header("pm-storage-token", "st-1"))
            .and(wiremock::matchers::body_string_contains("name=\"Block\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "Code": 1000 })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/storage/blocks/1"))
            .and(header("pm-storage-token", "st-2"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1, 2, 3]))
            .mount(&server)
            .await;
        let api = client(&server);
        let base = server.uri();
        api.upload_block(
            &format!("{base}/storage/blocks"),
            &Secret::new("st-1"),
            vec![9; 10],
        )
        .await
        .expect("upload");
        let bytes = api
            .download_block(&format!("{base}/storage/blocks/1"), &Secret::new("st-2"))
            .await
            .expect("download");
        assert_eq!(bytes, [1, 2, 3]);
    }

    thread_local! {
        static CAPTURED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    /// Sammelt alle `tracing`-Ausgaben (alle Level) des aktuellen Test-Threads.
    ///
    /// Ein globaler Subscriber statt `set_default`: Scoped Subscriber vertragen sich nicht mit
    /// dem Callsite-Cache, wenn parallele Tests ohne Subscriber dieselben Callsites treffen.
    /// Tests mit Capture laufen daher auf `current_thread`-Runtimes.
    #[derive(Clone, Copy, Default)]
    pub(crate) struct LogCapture;

    impl LogCapture {
        pub(crate) fn install() {
            static INIT: Once = Once::new();
            INIT.call_once(|| {
                let subscriber = tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::TRACE)
                    .with_ansi(false)
                    .with_writer(|| LogCapture)
                    .finish();
                tracing::subscriber::set_global_default(subscriber).expect("global subscriber");
            });
            CAPTURED.with(|c| c.borrow_mut().clear());
        }

        pub(crate) fn text() -> String {
            CAPTURED.with(|c| String::from_utf8_lossy(&c.borrow()).into_owned())
        }
    }

    impl io::Write for LogCapture {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            CAPTURED.with(|c| c.borrow_mut().extend_from_slice(buf));
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn logs_never_contain_tokens_paths_or_bodies() {
        LogCapture::install();
        let server = MockServer::start().await;
        Mock::given(path("/auth/v4/sessions/forks/SELECTOR-SECRET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Code": 1000, "AccessToken": "ACCESS-SECRET", "RefreshToken": "REFRESH-SECRET",
            })))
            .mount(&server)
            .await;
        Mock::given(path("/drive/shares/s/files"))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "Code": 2500, "Error": "geheimer-dateiname.seg existiert bereits",
            })))
            .mount(&server)
            .await;
        let api = client(&server);
        let token = Secret::new("BEARER-SECRET");
        let auth = Auth {
            uid: "UID-1",
            access_token: &token,
        };
        api.send(
            Request::get(
                "/auth/v4/sessions/forks/{selector}",
                "/auth/v4/sessions/forks/SELECTOR-SECRET",
            )
            .auth(auth),
        )
        .await
        .expect("fork");
        let err = api
            .send(
                Request::post("/drive/shares/{share}/files", "/drive/shares/s/files")
                    .json(json!({ "Name": "geheimer-dateiname.seg" }))
                    .auth(auth),
            )
            .await
            .unwrap_err();
        api.upload_block(
            &format!("{}/storage/STORAGE-PATH", server.uri()),
            &Secret::new("STORAGE-SECRET"),
            vec![0],
        )
        .await
        .unwrap_err();
        // Auch Debug-Ausgaben von Anfrage und Fehler bleiben sauber.
        tracing::debug!(?auth, error = %CoreError::from(err), "context");

        let text = LogCapture::text();
        assert!(
            text.contains("/auth/v4/sessions/forks/{selector}"),
            "{text}"
        );
        assert!(text.contains("code=2500"), "{text}");
        for secret in [
            "SELECTOR-SECRET",
            "ACCESS-SECRET",
            "REFRESH-SECRET",
            "BEARER-SECRET",
            "STORAGE-SECRET",
            "STORAGE-PATH",
            "geheimer-dateiname",
        ] {
            assert!(!text.contains(secret), "{secret} leaked: {text}");
        }
    }
}
