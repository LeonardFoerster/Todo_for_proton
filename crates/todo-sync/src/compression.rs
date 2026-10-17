//! zstd-Kompression der Segment- und Snapshot-Inhalte.
//!
//! Feature `zstd-native` (Standard): C-Bibliothek. Feature `pure-rust-zstd`: `ruzstd`
//! (z. B. wasm32). Beide erzeugen und lesen Standard-zstd-Frames – Geräte mit
//! unterschiedlichem Backend verstehen sich. Sind beide aktiv, gewinnt `zstd-native`.

use todo_core::{CoreError, CoreResult};

#[cfg(not(any(feature = "zstd-native", feature = "pure-rust-zstd")))]
compile_error!("todo-sync braucht das Feature `zstd-native` oder `pure-rust-zstd`");

/// Obergrenze für entpackte Daten (Schutz vor Dekompressionsbomben).
pub const MAX_DECOMPRESSED: usize = 64 * 1024 * 1024;

/// Kompressionsstufe der C-Bibliothek (schnell, für kleine Op-Listen ausreichend).
#[cfg(feature = "zstd-native")]
const LEVEL: i32 = 3;

/// # Errors
/// `CoreError::Internal`, wenn die Bibliothek scheitert (praktisch nie).
#[cfg(feature = "zstd-native")]
pub fn compress(data: &[u8]) -> CoreResult<Vec<u8>> {
    zstd::bulk::compress(data, LEVEL).map_err(|e| CoreError::Internal(format!("zstd: {e}")))
}

/// # Errors
/// `CoreError::Validation` bei ungültigen Daten oder mehr als [`MAX_DECOMPRESSED`] Bytes.
#[cfg(feature = "zstd-native")]
pub fn decompress(data: &[u8]) -> CoreResult<Vec<u8>> {
    zstd::bulk::decompress(data, MAX_DECOMPRESSED)
        .map_err(|e| CoreError::Validation(format!("invalid zstd data: {e}")))
}

/// # Errors
/// Nie (Signatur gleich zur nativen Variante).
#[cfg(all(feature = "pure-rust-zstd", not(feature = "zstd-native")))]
pub fn compress(data: &[u8]) -> CoreResult<Vec<u8>> {
    Ok(ruzstd::encoding::compress_to_vec(
        data,
        ruzstd::encoding::CompressionLevel::Fastest,
    ))
}

/// # Errors
/// `CoreError::Validation` bei ungültigen Daten oder mehr als [`MAX_DECOMPRESSED`] Bytes.
#[cfg(all(feature = "pure-rust-zstd", not(feature = "zstd-native")))]
pub fn decompress(data: &[u8]) -> CoreResult<Vec<u8>> {
    use std::io::Read;

    let invalid =
        |e: &dyn std::fmt::Display| CoreError::Validation(format!("invalid zstd data: {e}"));
    let decoder = ruzstd::decoding::StreamingDecoder::new(data).map_err(|e| invalid(&e))?;
    let mut out = Vec::new();
    decoder
        .take(MAX_DECOMPRESSED as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| invalid(&e))?;
    if out.len() > MAX_DECOMPRESSED {
        return Err(CoreError::Validation("decompressed data too large".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_shrinks_repetitive_data() {
        let data = b"Milch kaufen ".repeat(200);
        let packed = compress(&data).unwrap();
        assert!(packed.len() < data.len() / 4, "{} bytes", packed.len());
        assert_eq!(decompress(&packed).unwrap(), data);
        assert_eq!(decompress(&compress(&[]).unwrap()).unwrap(), b"");
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            decompress(b"not zstd at all"),
            Err(CoreError::Validation(_))
        ));
    }

    /// Fester zstd-Frame (`printf hallo | zstd --no-check`): beide Backends müssen ihn lesen.
    #[test]
    fn reads_standard_frames() {
        const FRAME: &[u8] = &[
            0x28, 0xb5, 0x2f, 0xfd, 0x00, 0x58, 0x29, 0x00, 0x00, 0x68, 0x61, 0x6c, 0x6c, 0x6f,
        ];
        assert_eq!(decompress(FRAME).unwrap(), b"hallo");
    }
}
