//! A ZIP writer, because a Tizen `.wgt` **is** a ZIP.
//!
//! Entries are stored, not deflated. That is deliberate and it is the right trade for
//! this crate:
//!
//! - Tizen's packager accepts stored entries, and the `.wgt` it produces is itself a
//!   plain ZIP with `config.xml` at the root.
//! - A stored archive needs no compression dependency in a crate whose whole job is
//!   layout and manifests, and no `zip` binary on the build host.
//! - The bytes are a pure function of the input: same payload in, same archive out.
//!   A packaged artifact you can diff and a test that can compare bytes both depend
//!   on that, and a deflate stream does not give it to you.
//!
//! Timestamps are fixed at the ZIP epoch (1980-01-01 00:00) for the same reason: a
//! build-time clock in the archive would make two identical builds differ.
//!
//! The writer is small enough to read in one sitting, which is the other requirement:
//! an artifact format that nobody audits is a supply-chain question waiting to be
//! asked.

/// The ZIP local-file and central-directory signatures.
const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;

/// Version needed to extract, and version made by: 2.0, stored entries.
const VERSION_20: u16 = 20;

/// General-purpose flag bit 11: file names are UTF-8.
///
/// Set on every entry rather than only on non-ASCII ones: a payload directory may
/// hold a non-ASCII name, and a flag that appears conditionally is a flag that is
/// wrong exactly once, on a wall nobody is looking at.
const FLAG_UTF8: u16 = 0x0800;

/// One file in the archive.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// Path inside the archive, `/`-separated, no leading slash.
    pub(crate) path: String,
    /// File contents.
    pub(crate) bytes: Vec<u8>,
}

/// Writes a ZIP archive with stored entries.
///
/// Returns the complete archive. The order of `entries` is preserved, which keeps the
/// output deterministic for a caller that sorts its input.
pub(crate) fn write_stored_zip(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.iter().map(|e| e.bytes.len() + 128).sum());
    let mut directory = Vec::with_capacity(entries.len() * 64);

    for entry in entries {
        let name = entry.path.as_bytes();
        let crc = crc32(&entry.bytes);
        let size = u32::try_from(entry.bytes.len()).unwrap_or(u32::MAX);
        let offset = u32::try_from(out.len()).unwrap_or(u32::MAX);

        out.extend_from_slice(&LOCAL_HEADER.to_le_bytes());
        out.extend_from_slice(&VERSION_20.to_le_bytes());
        out.extend_from_slice(&FLAG_UTF8.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0x0021u16.to_le_bytes()); // mod date: 1980-01-01
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra length
        out.extend_from_slice(name);
        out.extend_from_slice(&entry.bytes);

        directory.extend_from_slice(&CENTRAL_HEADER.to_le_bytes());
        directory.extend_from_slice(&VERSION_20.to_le_bytes()); // version made by
        directory.extend_from_slice(&VERSION_20.to_le_bytes()); // version needed
        directory.extend_from_slice(&FLAG_UTF8.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes()); // stored
        directory.extend_from_slice(&0u16.to_le_bytes()); // mod time
        directory.extend_from_slice(&0x0021u16.to_le_bytes()); // mod date
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes()); // extra length
        directory.extend_from_slice(&0u16.to_le_bytes()); // comment length
        directory.extend_from_slice(&0u16.to_le_bytes()); // disk number
        directory.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        directory.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name);
    }

    let directory_offset = u32::try_from(out.len()).unwrap_or(u32::MAX);
    let directory_size = u32::try_from(directory.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&directory);

    let count = u16::try_from(entries.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&END_OF_CENTRAL_DIRECTORY.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with the directory
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out
}

/// CRC-32 (IEEE 802.3), as ZIP requires.
///
/// Computed bitwise rather than through a 256-entry table: the table would be four
/// times the code for a speed this crate does not need, since the payloads it
/// packages are megabytes and the cost is a fraction of reading them from disk.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, bytes: &[u8]) -> Entry {
        Entry {
            path: path.to_string(),
            bytes: bytes.to_vec(),
        }
    }

    /// Reads back the end-of-central-directory record, the way a reader would.
    fn eocd(archive: &[u8]) -> (u16, u32, u32) {
        let at = archive.len() - 22;
        assert_eq!(
            u32::from_le_bytes(archive[at..at + 4].try_into().unwrap()),
            END_OF_CENTRAL_DIRECTORY
        );
        let count = u16::from_le_bytes(archive[at + 10..at + 12].try_into().unwrap());
        let size = u32::from_le_bytes(archive[at + 12..at + 16].try_into().unwrap());
        let offset = u32::from_le_bytes(archive[at + 16..at + 20].try_into().unwrap());
        (count, size, offset)
    }

    #[test]
    fn crc32_matches_the_published_check_value() {
        // The standard check value for CRC-32/IEEE: "123456789" -> 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn an_archive_has_a_readable_directory() {
        let archive = write_stored_zip(&[
            entry("config.xml", b"<widget/>"),
            entry("index.html", b"hi"),
        ]);
        let (count, size, offset) = eocd(&archive);
        assert_eq!(count, 2);
        assert_eq!(size as usize, archive.len() - 22 - offset as usize);
        assert_eq!(
            u32::from_le_bytes(
                archive[offset as usize..offset as usize + 4]
                    .try_into()
                    .unwrap()
            ),
            CENTRAL_HEADER
        );
    }

    #[test]
    fn stored_entries_carry_their_bytes_verbatim() {
        let payload = b"<widget xmlns=\"http://www.w3.org/ns/widgets\"/>";
        let archive = write_stored_zip(&[entry("config.xml", payload)]);
        let name_len = u16::from_le_bytes(archive[26..28].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(archive[18..22].try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(archive[14..18].try_into().unwrap());
        let data_at = 30 + name_len;
        assert_eq!(size, payload.len());
        assert_eq!(crc, crc32(payload));
        assert_eq!(&archive[data_at..data_at + size], payload);
    }

    #[test]
    fn the_archive_is_a_pure_function_of_its_entries() {
        let one = write_stored_zip(&[entry("a.txt", b"a"), entry("b.txt", b"b")]);
        let two = write_stored_zip(&[entry("a.txt", b"a"), entry("b.txt", b"b")]);
        assert_eq!(one, two);
    }

    #[test]
    fn an_empty_archive_is_still_an_archive() {
        let archive = write_stored_zip(&[]);
        assert_eq!(archive.len(), 22);
        assert_eq!(eocd(&archive).0, 0);
    }
}
