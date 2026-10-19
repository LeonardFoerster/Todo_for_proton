//! Login-URL an den Browser übergeben, ohne sie in eine Kommandozeile zu schreiben.
//!
//! Das Fragment der Login-URL enthält User-Code und Fork-Schlüssel. Als Argument von `xdg-open`
//! stünde es in `/proc/<pid>/cmdline`, das andere Nutzer des Systems lesen können; wer es mitliest,
//! könnte den Fork mit einem eigenen Konto bestätigen (Login-Fixation). Stattdessen geht nur der
//! Pfad einer nur für den Besitzer lesbaren HTML-Datei an den Browser, die weiterleitet.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use todo_core::{CoreError, CoreResult};

/// Schreibt die Weiterleitungsseite nach `$XDG_RUNTIME_DIR` (nur für den Nutzer, tmpfs) bzw. ins
/// Temp-Verzeichnis und liefert ihren Pfad. Der Aufrufer öffnet sie und löscht sie danach.
///
/// # Errors
/// `CoreError::Internal`, wenn die Datei nicht angelegt werden kann.
pub fn write_login_redirect(url: &str) -> CoreResult<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute() && d.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    write_in(&dir, url)
}

fn write_in(dir: &Path, url: &str) -> CoreResult<PathBuf> {
    let failed = |e: std::io::Error| CoreError::Internal(format!("login page: {e}"));
    let mut name = [0u8; 16];
    getrandom::fill(&mut name).map_err(|e| CoreError::Internal(format!("random: {e}")))?;
    let path = dir.join(format!("tasks-login-{}.html", hex::encode(name)));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(failed)?;
    file.write_all(page(url).as_bytes()).map_err(|e| {
        let _ = fs::remove_file(&path);
        failed(e)
    })?;
    Ok(path)
}

fn page(url: &str) -> String {
    let url = escape(url);
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\
         <meta name=\"referrer\" content=\"no-referrer\">\
         <meta http-equiv=\"refresh\" content=\"0;url={url}\">\
         <title>Tasks – Anmeldung</title></head>\
         <body><p><a href=\"{url}\">Weiter zur Anmeldung</a></p></body></html>\n"
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_is_private_and_redirects_to_the_escaped_url() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://account.proton.me/desktop/login?app=drive&pv=3#payload=0%3Acode%3Akey";
        let path = write_in(dir.path(), url).unwrap();
        let html = fs::read_to_string(&path).unwrap();
        assert!(html.contains(
            "url=https://account.proton.me/desktop/login?app=drive&amp;pv=3#payload=0%3Acode%3Akey\""
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // Zweite Seite: neuer, nicht vorhersagbarer Name.
        assert_ne!(write_in(dir.path(), url).unwrap(), path);
    }

    #[test]
    fn markup_in_url_is_escaped() {
        assert!(!page("x\"><script>").contains("<script>"));
    }
}
