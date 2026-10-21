//! Eine Instanz je Profil (ADR-033): Läuft die App schon (z. B. im Tray), meldet sich eine zweite über
//! einen Unix-Socket („zeigen“) und beendet sich – zwei Prozesse auf derselben Datenbank darf es nicht
//! geben. Der Socket liegt im Laufzeitordner des Benutzers (`$XDG_RUNTIME_DIR`, Name aus dem Datenordner
//! abgeleitet – Socket-Pfade dürfen nur ~100 Zeichen lang sein). Der wartende Thread schläft in `accept`.

use std::{
    io::Write,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
};

/// Socket für den Datenordner `dir` (je Profil einer).
fn socket_path(dir: &Path) -> PathBuf {
    let runtime =
        std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    // FNV-1a: stabil über Builds hinweg (anders als `DefaultHasher`).
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in dir.as_os_str().as_encoded_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
    }
    runtime.join(format!("dev.tasks.app-{hash:016x}.sock"))
}

/// Ergebnis von [`claim`].
#[derive(Debug)]
pub enum Instance {
    /// Diese ist die erste; `listen` nimmt Meldungen weiterer Starts an.
    First(Option<UnixListener>),
    /// Eine andere läuft schon und wurde benachrichtigt.
    Other,
}

/// Prüft, ob schon eine Instanz läuft (und weckt sie), sonst übernimmt diese den Socket.
#[must_use]
pub fn claim(dir: &Path) -> Instance {
    let path = socket_path(dir);
    if let Ok(mut stream) = UnixStream::connect(&path) {
        let _ = stream.write_all(b"show\n");
        return Instance::Other;
    }
    // Verwaister Socket einer abgestürzten Instanz.
    let _ = std::fs::remove_file(&path);
    match UnixListener::bind(&path) {
        Ok(listener) => Instance::First(Some(listener)),
        Err(err) => {
            tracing::warn!(error = %err, "single-instance socket not available");
            Instance::First(None)
        }
    }
}

/// Ruft `on_show` für jede weitere gestartete Instanz auf (eigener Thread).
pub fn listen(listener: UnixListener, on_show: impl Fn() + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("single-instance".into())
        .spawn(move || {
            for stream in listener.incoming() {
                if stream.is_ok() {
                    on_show();
                }
            }
        });
    if let Err(err) = spawned {
        tracing::warn!(error = %err, "single-instance thread");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use super::*;

    #[test]
    fn second_start_wakes_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let Instance::First(Some(listener)) = claim(dir.path()) else {
            panic!("erste Instanz erwartet");
        };
        let shown = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&shown);
        listen(listener, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert!(matches!(claim(dir.path()), Instance::Other));
        for _ in 0..100 {
            if shown.load(Ordering::SeqCst) == 1 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("erste Instanz nicht geweckt");
    }
}
