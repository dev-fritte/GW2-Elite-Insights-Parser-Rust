//! Loading log files from disk.
//!
//! Logs are read into one buffer which [`crate::Evtc::parse`] then borrows from. Memory mapping
//! is deliberately not used here: it needs `unsafe`, compressed logs (`.zevtc`) have to be
//! inflated into memory anyway, and plain `.evtc` files are rare. It can be added behind a
//! feature if profiling asks for it.

use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use zip::ZipArchive;

use crate::error::Error;

/// `true` for the compressed formats the C# parser accepts (`SupportedFileFormats`):
/// names ending in `.zevtc` or `.evtc.zip`, compared case-insensitively.
#[must_use]
pub fn is_compressed_name(name: &str) -> bool {
    ends_with_ignore_case(name, ".zevtc") || ends_with_ignore_case(name, ".evtc.zip")
}

/// `true` for every name the C# parser accepts: the compressed formats and `.evtc`.
#[must_use]
pub fn is_supported_name(name: &str) -> bool {
    is_compressed_name(name) || ends_with_ignore_case(name, ".evtc")
}

fn ends_with_ignore_case(name: &str, suffix: &str) -> bool {
    let (name, suffix) = (name.as_bytes(), suffix.as_bytes());
    name.len() >= suffix.len() && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// Reads the single entry of a ZIP archive into memory.
///
/// # Errors
/// [`Error::InvalidArchive`] if the data is not a ZIP archive or does not hold exactly one
/// entry (the same rule as the C# reader), [`Error::Io`] if inflating fails.
pub fn read_archive<R: Read + Seek>(source: R) -> Result<Vec<u8>, Error> {
    let mut archive = ZipArchive::new(source).map_err(|e| Error::InvalidArchive(e.to_string()))?;
    if archive.len() != 1 {
        return Err(Error::InvalidArchive(format!(
            "expected 1 entry, found {}",
            archive.len()
        )));
    }
    let mut entry = archive
        .by_index(0)
        .map_err(|e| Error::InvalidArchive(e.to_string()))?;
    // The size is only a hint taken from the archive, cap it so a hostile header cannot
    // make us reserve gigabytes up front.
    let hint = usize::try_from(entry.size()).unwrap_or(0).min(1 << 30);
    let mut data = Vec::with_capacity(hint);
    entry.read_to_end(&mut data)?;
    Ok(data)
}

/// Reads a log file into memory. Compressed formats are inflated, anything else is read as is.
///
/// # Errors
/// [`Error::Io`] if the file cannot be read, [`Error::InvalidArchive`] for broken archives.
pub fn read_log(path: &Path) -> Result<Vec<u8>, Error> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if is_compressed_name(name) {
        read_archive(File::open(path)?)
    } else {
        Ok(std::fs::read(path)?)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn recognises_log_names() {
        assert!(is_compressed_name("20260928-202622.zevtc"));
        assert!(is_compressed_name("log.EVTC.ZIP"));
        assert!(!is_compressed_name("log.evtc"));
        assert!(is_supported_name("log.EVTC"));
        assert!(!is_supported_name("log.txt"));
    }

    #[test]
    fn reads_the_only_entry() {
        let zip = archive(&[("log.evtc", b"EVTC1234")]);
        assert_eq!(read_archive(Cursor::new(zip)).unwrap(), b"EVTC1234");
    }

    #[test]
    fn rejects_archives_without_exactly_one_entry() {
        assert!(matches!(
            read_archive(Cursor::new(archive(&[]))),
            Err(Error::InvalidArchive(_))
        ));
        assert!(matches!(
            read_archive(Cursor::new(archive(&[("a", b"1"), ("b", b"2")]))),
            Err(Error::InvalidArchive(_))
        ));
        assert!(matches!(
            read_archive(Cursor::new(b"not a zip".to_vec())),
            Err(Error::InvalidArchive(_))
        ));
    }
}
