use std::fmt;

use oo7::{dbus, Keyring};
use todo_core::{
    ports::{KeyStore, SecretBytes},
    CoreError, CoreResult,
};

/// Attribut, über das unsere Einträge gefunden werden (zusätzlich zu [`ATTR_KEY`]).
const ATTR_APP: &str = "application";
const ATTR_KEY: &str = "key";

/// Einträge im Standard-Schlüsselbund („Login"), je App-Kennung getrennt.
///
/// Jeder Eintrag trägt die Attribute `application = <app_id>` und `key = <name>`; das Label
/// (sichtbar z. B. in Seahorse/KWalletManager) ist `<app_label>: <name>`.
pub struct SecretServiceKeyStore {
    keyring: Keyring,
    app_id: String,
    app_label: String,
}

impl fmt::Debug for SecretServiceKeyStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretServiceKeyStore")
            .field("app_id", &self.app_id)
            .finish_non_exhaustive()
    }
}

impl SecretServiceKeyStore {
    /// Verbindet sich mit dem Secret Service (bzw. im Flatpak mit dem Secret-Portal).
    ///
    /// # Errors
    /// `CoreError::Storage` mit verständlichem Hinweis, wenn kein Dienst erreichbar ist.
    pub async fn connect(app_id: &str, app_label: &str) -> CoreResult<Self> {
        let keyring = Keyring::new().await.map_err(|e| connect_error(&e))?;
        Ok(Self {
            keyring,
            app_id: app_id.to_owned(),
            app_label: app_label.to_owned(),
        })
    }

    fn attributes<'a>(&'a self, name: &'a str) -> [(&'a str, &'a str); 2] {
        [(ATTR_APP, self.app_id.as_str()), (ATTR_KEY, name)]
    }

    /// Ein gesperrter Schlüsselbund wird entsperrt (ggf. mit Passwortdialog des Systems).
    async fn unlocked(&self) -> CoreResult<&Keyring> {
        if self.keyring.is_locked().await.map_err(|e| error(&e))? {
            self.keyring.unlock().await.map_err(|e| error(&e))?;
        }
        Ok(&self.keyring)
    }
}

#[async_trait::async_trait]
impl KeyStore for SecretServiceKeyStore {
    async fn get(&self, name: &str) -> CoreResult<Option<SecretBytes>> {
        let keyring = self.unlocked().await?;
        let items = keyring
            .search_items(&self.attributes(name))
            .await
            .map_err(|e| error(&e))?;
        let Some(item) = items.into_iter().next() else {
            return Ok(None);
        };
        if item.is_locked().await.map_err(|e| error(&e))? {
            item.unlock().await.map_err(|e| error(&e))?;
        }
        let secret = item.secret().await.map_err(|e| error(&e))?;
        Ok(Some(SecretBytes::new(secret.as_bytes().to_vec())))
    }

    async fn put(&self, name: &str, value: SecretBytes) -> CoreResult<()> {
        let keyring = self.unlocked().await?;
        let label = format!("{}: {name}", self.app_label);
        keyring
            .create_item(
                &label,
                &self.attributes(name),
                oo7::Secret::blob(value.expose()),
                true,
            )
            .await
            .map_err(|e| error(&e))
    }

    async fn delete(&self, name: &str) -> CoreResult<()> {
        let keyring = self.unlocked().await?;
        keyring
            .delete(&self.attributes(name))
            .await
            .map_err(|e| error(&e))
    }
}

fn connect_error(err: &oo7::Error) -> CoreError {
    CoreError::Storage(format!(
        "system keyring not reachable (freedesktop Secret Service, e.g. GNOME Keyring or \
         KWallet; D-Bus name org.freedesktop.secrets): {}",
        describe(err)
    ))
}

fn error(err: &oo7::Error) -> CoreError {
    CoreError::Storage(format!("system keyring: {}", describe(err)))
}

fn describe(err: &oo7::Error) -> String {
    match err {
        oo7::Error::DBus(dbus::Error::Dismissed) => "unlock prompt was dismissed".to_owned(),
        oo7::Error::DBus(dbus::Error::NotFound(_)) => "no default collection".to_owned(),
        other => other.to_string(),
    }
}
