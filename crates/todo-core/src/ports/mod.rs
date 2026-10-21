//! Ports (Traits), über die der Core mit der Außenwelt spricht.
//!
//! Implementierungen liegen ausschließlich in Adapter-Crates.
//!
//! # Async-Ports
//!
//! Async-Ports nutzen `async-trait`, damit sie als `Arc<dyn Port>` verwendbar bleiben. Nativ
//! (tokio/Tauri) müssen ihre Futures `Send` sein, im Browser (wasm32, single-threaded) nicht.
//! Jeder Port und jede Implementierung trägt deshalb dieses Attributpaar:
//!
//! ```
//! use todo_core::{ports::{MaybeSend, MaybeSync}, CoreResult};
//!
//! #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
//! #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
//! pub trait Echo: MaybeSend + MaybeSync {
//!     async fn echo(&self, text: String) -> CoreResult<String>;
//! }
//! ```
//!
//! Bewusst kein eigenes `#[port]`-Makro: Ein `macro_rules!`-Wrapper um den ganzen Trait würde
//! rustfmt am Formatieren des Inhalts hindern.

mod auth;
mod cipher;
mod clock;
mod note_repository;
mod remote_store;
mod repository;
mod time_zones;

pub use auth::{AuthError, AuthFlowId, AuthProvider, AuthStep, SessionInfo};
pub use cipher::{Cipher, KeyStore, SecretBytes};
pub use clock::Clock;
#[cfg(any(test, feature = "test-support"))]
pub use clock::{FixedClock, SteppingClock};
pub use note_repository::{
    check_page_limit, decode_tree_cursor, encode_tree_cursor, visible_folders, FolderItemsQuery,
    NoteHit, NoteRepository, NoteSnippet, NoteSummary, NoteTreeItem, NOTE_SEARCH_MIN_BODY_TERM,
};
pub use remote_store::{
    RemoteChanges, RemoteCursor, RemoteDir, RemoteEntry, RemoteListing, RemotePath, RemoteStore,
    Revision,
};
pub use repository::{
    Change, ChangeSet, CommitReceipt, EntityRef, Page, PageRequest, SearchQuery, SmartView,
    TaskOrder, TaskQuery, TaskRepository, TaskSort, TaskSummary, PAGE_LIMIT_MAX,
    SEARCH_TEXT_MAX_CHARS,
};
#[cfg(any(test, feature = "test-support"))]
pub use time_zones::FixedOffsetZones;
pub use time_zones::TimeZones;

/// `Send` auf nativen Zielen, leer auf wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> MaybeSend for T {}

/// `Send` auf nativen Zielen, leer auf wasm32.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

/// `Sync` auf nativen Zielen, leer auf wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSync: Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync + ?Sized> MaybeSync for T {}

/// `Sync` auf nativen Zielen, leer auf wasm32.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSync for T {}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    use futures::executor::block_on;

    use super::*;
    use crate::{CoreError, CoreResult};

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    trait Echo: MaybeSend + MaybeSync {
        async fn echo(&self, text: String) -> CoreResult<String>;
    }

    #[derive(Debug, Default)]
    struct CountingEcho {
        calls: AtomicU32,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl Echo for CountingEcho {
        async fn echo(&self, text: String) -> CoreResult<String> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if text.is_empty() {
                return Err(CoreError::Validation("empty".into()));
            }
            Ok(text)
        }
    }

    #[test]
    fn dummy_port_runs_through_dyn() {
        let port: Arc<dyn Echo> = Arc::new(CountingEcho::default());
        assert_eq!(block_on(port.echo("hallo".into())).unwrap(), "hallo");
        assert!(matches!(
            block_on(port.echo(String::new())),
            Err(CoreError::Validation(_))
        ));
    }

    #[test]
    fn implementation_state_is_shared() {
        let echo = Arc::new(CountingEcho::default());
        let port: Arc<dyn Echo> = echo.clone();
        block_on(port.echo("a".into())).unwrap();
        block_on(port.echo("b".into())).unwrap();
        assert_eq!(echo.calls.load(Ordering::Relaxed), 2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_port_futures_and_handles_are_send() {
        fn assert_send<T: Send>(_: &T) {}
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        let port: Arc<dyn Echo> = Arc::new(CountingEcho::default());
        let fut = port.echo("x".into());
        assert_send(&fut);
        drop(fut);
        assert_send_sync::<dyn Echo>();
        assert_send_sync::<dyn Clock>();
    }

    #[test]
    fn clock_is_usable_as_shared_trait_object() {
        let clock: Arc<dyn Clock> = Arc::new(FixedClock(crate::domain::Timestamp::EPOCH));
        assert_eq!(clock.now(), crate::domain::Timestamp::EPOCH);
    }
}
