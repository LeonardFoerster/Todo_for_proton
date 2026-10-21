//! Typsichere IDs als Newtypes über UUIDv7.
//!
//! An allen Grenzen (IPC, Speicher, Sync) werden IDs als String transportiert.
//! Die Typen sind untereinander nicht austauschbar:
//!
//! ```compile_fail
//! use todo_core::domain::{TaskId, TaskListId};
//! let list_id: TaskListId = TaskId::new();
//! ```

use std::{fmt, str::FromStr};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Ein String ließ sich nicht als ID lesen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIdError {
    input: String,
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid id: {:?}", self.input)
    }
}

impl std::error::Error for ParseIdError {}

/// Liest eine UUID aus 16 Bytes (Binärformate).
struct UuidBytes;

impl<'de> de::Visitor<'de> for UuidBytes {
    type Value = Uuid;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("16 id bytes")
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Uuid, E> {
        Uuid::from_slice(v).map_err(E::custom)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Uuid, A::Error> {
        let mut bytes = Vec::with_capacity(16);
        while let Some(byte) = seq.next_element::<u8>()? {
            bytes.push(byte);
        }
        self.visit_bytes(&bytes)
    }
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Uuid);

        impl $name {
            /// Erzeugt eine neue, zeitlich sortierbare ID (UUIDv7).
            // Kein `Default`: eine „Standard-ID" wäre irreführend, jede ID ist neu.
            #[allow(clippy::new_without_default)]
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Liest eine ID aus ihrer String-Darstellung.
            ///
            /// # Errors
            /// [`ParseIdError`], wenn `s` keine gültige UUID ist.
            pub fn parse(s: &str) -> Result<Self, ParseIdError> {
                Uuid::try_parse(s).map(Self).map_err(|_| ParseIdError { input: s.to_owned() })
            }

            /// Die 16 Bytes der UUID (Big Endian; Byte-Reihenfolge = Sortierreihenfolge).
            #[must_use]
            pub fn to_bytes(self) -> [u8; 16] {
                self.0.into_bytes()
            }

            /// Gegenstück zu [`Self::to_bytes`] (kompakte Binärformate, z. B. Sync).
            #[must_use]
            pub fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(Uuid::from_bytes(bytes))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.hyphenated().fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = ParseIdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        /// Menschenlesbare Formate (JSON) bekommen den String, Binärformate (Sync) 16 Bytes.
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                if serializer.is_human_readable() {
                    serializer.collect_str(self)
                } else {
                    serializer.serialize_bytes(self.0.as_bytes())
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                if deserializer.is_human_readable() {
                    let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                    Self::parse(&s).map_err(de::Error::custom)
                } else {
                    deserializer.deserialize_bytes(UuidBytes).map(Self)
                }
            }
        }
    };
}

define_id!(
    /// ID einer [`Task`](crate::domain::Task).
    TaskId
);
define_id!(
    /// ID einer Liste ([`TaskList`](crate::domain::TaskList)).
    TaskListId
);

impl TaskListId {
    /// Feste ID der Inbox – auf allen Geräten gleich, damit deren Inboxen beim Sync zu einer
    /// zusammenlaufen statt sich zu verdoppeln (ADR-015).
    pub const INBOX: Self = Self(Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001));
}
define_id!(
    /// ID eines `Tag`s.
    TagId
);
define_id!(
    /// ID eines Geräts (eine Installation der App).
    DeviceId
);
define_id!(
    /// ID einer Notiz ([`Note`](crate::domain::Note)).
    NoteId
);
define_id!(
    /// ID eines Notiz-Ordners ([`NoteFolder`](crate::domain::NoteFolder)).
    NoteFolderId
);

impl NoteId {
    /// Deterministische ID (UUIDv5 im Namensraum der Notiz-IDs) – z. B. für Konfliktkopien, die
    /// mehrere Geräte unabhängig voneinander anlegen und die zu einer zusammenlaufen müssen
    /// (ADR-029).
    #[must_use]
    pub fn derived(parts: &[&[u8]]) -> Self {
        let name: Vec<u8> = parts.concat();
        Self(Uuid::new_v5(&NOTE_ID_NAMESPACE, &name))
    }
}

impl TaskId {
    /// Deterministische ID (UUIDv5) – Verlaufseintrag einer Serie (ADR-028): Zwei Geräte, die
    /// offline dasselbe Vorkommen abhaken, erzeugen dieselbe ID und damit **einen** Eintrag.
    #[must_use]
    pub fn derived(parts: &[&[u8]]) -> Self {
        let name: Vec<u8> = parts.concat();
        Self(Uuid::new_v5(&TASK_ID_NAMESPACE, &name))
    }
}

/// Fester Namensraum für [`TaskId::derived`]. **Nie ändern** (sonst doppeln sich Verlaufseinträge).
const TASK_ID_NAMESPACE: Uuid = Uuid::from_u128(0x7461_736b_2d6f_6363_7572_0000_0000_0001);

/// Fester Namensraum für [`NoteId::derived`]. **Nie ändern** (sonst entstehen neue Kopien).
const NOTE_ID_NAMESPACE: Uuid = Uuid::from_u128(0x6e6f_7465_2d63_6f70_7900_0000_0000_0001);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_roundtrip() {
        let id = TaskId::new();
        let s = id.to_string();
        assert_eq!(TaskId::parse(&s).unwrap(), id);
        assert_eq!(s.parse::<TaskId>().unwrap(), id);
    }

    #[test]
    fn display_is_lowercase_hyphenated() {
        let id = TaskListId::parse("0192F0C4-7A3B-7C2D-8E4F-0123456789AB").unwrap();
        assert_eq!(id.to_string(), "0192f0c4-7a3b-7c2d-8e4f-0123456789ab");
    }

    #[test]
    fn new_ids_are_distinct() {
        assert_ne!(TagId::new(), TagId::new());
    }

    #[test]
    fn new_ids_are_time_sortable() {
        let ids: Vec<DeviceId> = (0..1_000).map(|_| DeviceId::new()).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        let mut strings: Vec<String> = ids.iter().map(ToString::to_string).collect();
        let created = strings.clone();
        strings.sort();
        assert_eq!(
            strings, created,
            "String-Sortierung entspricht Erzeugungsreihenfolge"
        );
    }

    #[test]
    fn invalid_string_is_rejected() {
        for bad in ["", "not-a-uuid", "0192f0c4-7a3b-7c2d-8e4f-0123456789a"] {
            let err = TaskId::parse(bad).unwrap_err();
            assert!(err.to_string().contains("invalid id"));
        }
    }

    #[test]
    fn bytes_roundtrip_and_keep_order() {
        let a = DeviceId::new();
        let b = DeviceId::new();
        assert_eq!(DeviceId::from_bytes(a.to_bytes()), a);
        assert_eq!(a.to_bytes() < b.to_bytes(), a < b);
    }

    #[test]
    fn derived_note_ids_are_stable_and_distinct() {
        let a = NoteId::derived(&[b"x", b"y"]);
        assert_eq!(a, NoteId::derived(&[b"x", b"y"]));
        assert_ne!(a, NoteId::derived(&[b"x", b"z"]));
        assert_eq!(a.to_string(), NoteId::derived(&[b"xy"]).to_string());
        assert_eq!(
            NoteId::derived(&[b"probe"]).to_string(),
            "fc60b84d-5926-5d3f-99c2-a9440cb1a4f9",
            "Namensraum unverändert"
        );
    }

    #[test]
    fn serializes_as_plain_string() {
        let id = TaskId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<TaskId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<TaskId>("\"nope\"").is_err());
        assert!(serde_json::from_str::<TaskId>("42").is_err());
    }
}
