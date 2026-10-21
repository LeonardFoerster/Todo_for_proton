//! Einstellungen des Hintergrundbetriebs (ADR-033): je Gerät in `settings.json` im Datenordner, nicht
//! synchronisiert. Fehlende oder kaputte Datei = Standard.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::plan::parse_all_day_time;

const FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", default)]
pub struct BackgroundSettingsDto {
    /// Erinnerungen an Fälligkeiten.
    pub reminders: bool,
    /// Uhrzeit für ganztägige Fälligkeiten, `"HH:MM"`.
    pub all_day_time: String,
    /// Schließen versteckt das Fenster, die App bleibt im Tray.
    pub background: bool,
    /// Mit dem System starten (versteckt, im Tray).
    pub autostart: bool,
}

impl Default for BackgroundSettingsDto {
    fn default() -> Self {
        Self {
            reminders: true,
            all_day_time: "09:00".into(),
            background: false,
            autostart: false,
        }
    }
}

impl BackgroundSettingsDto {
    /// Uhrzeit als `(Stunde, Minute)`; ungültig → 09:00.
    #[must_use]
    pub fn all_day(&self) -> (i8, i8) {
        parse_all_day_time(&self.all_day_time).unwrap_or((9, 0))
    }

    /// # Errors
    /// Ungültige Uhrzeit; Autostart ohne Hintergrundbetrieb ergibt keinen Sinn.
    pub fn validate(&self) -> Result<(), String> {
        parse_all_day_time(&self.all_day_time)?;
        if self.autostart && !self.background {
            return Err("autostart needs background mode".into());
        }
        Ok(())
    }
}

/// Lädt die Einstellungen (Standard, wenn nicht lesbar).
#[must_use]
pub fn load(dir: &Path) -> BackgroundSettingsDto {
    std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<BackgroundSettingsDto>(&bytes).ok())
        .filter(|s| s.validate().is_ok())
        .unwrap_or_default()
}

/// # Errors
/// Schreibfehler.
pub fn save(dir: &Path, settings: &BackgroundSettingsDto) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(FILE)).map_err(|e| e.to_string())
}

/// `~/.config/autostart/<app-id>.desktop` (XDG Autostart).
fn autostart_file(app_id: &str) -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("autostart").join(format!("{app_id}.desktop")))
}

/// Legt den Autostart-Eintrag an (startet versteckt im Tray) bzw. entfernt ihn.
///
/// # Errors
/// Schreib-/Löschfehler.
pub fn set_autostart(
    app_id: &str,
    enabled: bool,
    exe: &Path,
    data_dir: Option<&Path>,
) -> Result<(), String> {
    let file = autostart_file(app_id).ok_or("no config directory")?;
    if !enabled {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Ein eigenes Profil (`TODO_DATA_DIR`) startet mit demselben Profil.
    let env = data_dir.map_or(String::new(), |d| {
        format!("env TODO_DATA_DIR={} ", quote(&d.to_string_lossy()))
    });
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=Tasks\nComment=Tasks im Hintergrund (Erinnerungen)\nExec={env}{} --hidden\nIcon={app_id}\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
        quote(&exe.to_string_lossy())
    );
    std::fs::write(&file, content).map_err(|e| e.to_string())
}

/// Argument für `Exec=` (Desktop-Entry-Spezifikation): in Anführungszeichen, `"`, `` ` ``, `$`, `\` maskiert.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip_and_validation() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), BackgroundSettingsDto::default());
        let s = BackgroundSettingsDto {
            reminders: false,
            all_day_time: "07:30".into(),
            background: true,
            autostart: true,
        };
        save(dir.path(), &s).unwrap();
        assert_eq!(load(dir.path()), s);
        assert_eq!(s.all_day(), (7, 30));
        std::fs::write(dir.path().join(FILE), "{\"allDayTime\":\"25:00\"}").unwrap();
        assert_eq!(
            load(dir.path()),
            BackgroundSettingsDto::default(),
            "ungültig → Standard"
        );
        assert!(BackgroundSettingsDto {
            autostart: true,
            ..BackgroundSettingsDto::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn exec_arguments_are_quoted() {
        assert_eq!(quote("/opt/My App/tasks"), "\"/opt/My App/tasks\"");
        assert_eq!(quote("a\"$b"), "\"a\\\"\\$b\"");
    }
}
