//! Proton als Sync-Backend der Dev-CLI: Login per Browser, Sitzung und Sync-Schlüssel im
//! System-Schlüsselbund (Secret Service). Die CLI kennt nur die Adapter-Typen, keine Proton-Details.

use std::{process::Command as Process, sync::Arc, time::Duration};

use proton_adapter::{ApiConfig, ProtonAuthProvider, ProtonDriveRemoteStore};
use todo_core::{
    ports::{AuthError, AuthProvider, AuthStep, KeyStore, RemoteStore},
    CoreError,
};
use todo_keystore_secret_service::SecretServiceKeyStore;
use todo_sync::SyncConfig;

use crate::{Error, Result};

/// Ehrliche Kennung nach den Proton-SDK-Regeln (ADR-013). Kanal `alpha` ist live erprobt (P2).
const APP_VERSION: &str = "external-drive-tasks@0.1.0-alpha";
/// Eigene Einträge im Schlüsselbund, getrennt von der späteren Desktop-App.
const KEYRING_APP_ID: &str = "dev.tasks.todo-cli";
const KEYRING_LABEL: &str = "Tasks (Dev-CLI)";
const NOTICE: &str = "This is a third-party application not officially supported by Proton.";

pub struct Proton {
    pub auth: Arc<ProtonAuthProvider>,
    pub key_store: Arc<dyn KeyStore>,
}

impl Proton {
    pub async fn connect() -> Result<Self> {
        let key_store: Arc<dyn KeyStore> =
            Arc::new(SecretServiceKeyStore::connect(KEYRING_APP_ID, KEYRING_LABEL).await?);
        let auth = Arc::new(ProtonAuthProvider::new(
            ApiConfig::new(APP_VERSION),
            Arc::clone(&key_store),
        )?);
        Ok(Self { auth, key_store })
    }

    /// Store für `sync`; scheitert verständlich, wenn niemand angemeldet ist.
    pub async fn remote(&self) -> Result<Arc<dyn RemoteStore>> {
        match self.auth.resume().await {
            Ok(Some(_)) => Ok(Arc::new(ProtonDriveRemoteStore::new(&self.auth)?)),
            Ok(None) => Err(Error::Input(
                "nicht bei Proton angemeldet – zuerst `todo-cli login`".into(),
            )),
            Err(CoreError::Auth(AuthError::SessionRevoked)) => Err(Error::Input(
                "Proton-Sitzung abgelaufen oder widerrufen – neu mit `todo-cli login`".into(),
            )),
            Err(err) => Err(err.into()),
        }
    }
}

pub async fn login() -> Result<()> {
    println!("{NOTICE}");
    let proton = Proton::connect().await?;
    if let Some(info) = proton.auth.current_session().await? {
        println!(
            "Bereits angemeldet als {} (abmelden: todo-cli logout).",
            info.account_label
        );
        return Ok(());
    }
    let AuthStep::OpenBrowser {
        flow,
        url,
        poll_interval_s,
        expires_in_s,
    } = proton.auth.begin_login().await?
    else {
        return Err(Error::Input("unerwartete Antwort beim Login-Start".into()));
    };
    println!("Im Browser anmelden und den Zugriff bestätigen (2FA erledigt die Login-Seite):");
    println!("  {url}");
    println!(
        "Warte auf Bestätigung (höchstens {} min) …",
        expires_in_s / 60
    );
    // Browser öffnen, wenn möglich; sonst genügt die angezeigte URL.
    let _ = Process::new("xdg-open").arg(&url).spawn();
    loop {
        tokio::time::sleep(Duration::from_secs(u64::from(poll_interval_s))).await;
        match proton.auth.poll_login(&flow).await? {
            AuthStep::Pending { .. } => {}
            AuthStep::Done(info) => {
                println!("Angemeldet als {}.", info.account_label);
                return Ok(());
            }
            AuthStep::OpenBrowser { .. } => {
                return Err(Error::Input("unerwartete Antwort beim Login".into()));
            }
        }
    }
}

pub async fn logout() -> Result<()> {
    let proton = Proton::connect().await?;
    proton.auth.logout().await?;
    proton
        .key_store
        .delete(&SyncConfig::default().key_name())
        .await?;
    println!("Abgemeldet; Sitzung und Sync-Schlüssel aus dem Schlüsselbund entfernt.");
    Ok(())
}
