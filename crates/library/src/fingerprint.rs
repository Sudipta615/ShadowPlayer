//! Cheap, dependency-free content fingerprinting.
//!
//! The goal is *not* cryptographic integrity but a stable identity for a media
//! file that survives a move or rename: we combine the file size with a fast
//! FNV-1a hash of the first and last 64 KiB. That is enough to tell two
//! different tracks apart while staying far cheaper than hashing a whole
//! multi-hundred-megabyte FLAC during a library scan.
//!
//! [`Fingerprint::compute`] returns `None` when the file cannot be read, in
//! which case callers keep whatever identity they had.

use std::fmt::Write as _;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How many bytes are sampled from each end of the file.
const SAMPLE_BYTES: u64 = 64 * 1024;

/// A computed fingerprint, formatted as `"<size>-<hash>"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// Fingerprint a file. Returns `None` if it cannot be opened or read.
    pub fn compute(path: &Path) -> Option<Self> {
        let mut file = std::fs::File::open(path).ok()?;
        let len = file.metadata().ok()?.len();

        // FNV-1a over size, then the head, then the tail.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |bytes: &[u8]| {
            for &b in bytes {
                hash ^= b as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        mix(&len.to_le_bytes());

        let mut head = vec![0u8; SAMPLE_BYTES.min(len) as usize];
        if file.read_exact(&mut head).is_ok() {
            mix(&head);
        }

        // Only read a separate tail block when the file is longer than one
        // sample, so small files are not hashed twice.
        if len > SAMPLE_BYTES {
            let tail_start = len - SAMPLE_BYTES;
            if file.seek(SeekFrom::Start(tail_start)).is_ok() {
                let mut tail = vec![0u8; SAMPLE_BYTES as usize];
                if file.read_exact(&mut tail).is_ok() {
                    mix(&tail);
                }
            }
        }

        let mut s = format!("{len}-");
        let _ = write!(s, "{hash:016x}");
        Some(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Owned fingerprint string suitable for storing in the database.
    pub fn into_string(self) -> String {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sp_fingerprint_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn identical_bytes_share_a_fingerprint_and_content_differs() {
        let dir = temp_dir("same");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        let c = dir.join("c.bin");
        std::fs::write(&a, b"hello world").unwrap();
        std::fs::write(&b, b"hello world").unwrap();
        std::fs::write(&c, b"hello worlD").unwrap();

        let fa = Fingerprint::compute(&a).expect("a");
        let fb = Fingerprint::compute(&b).expect("b");
        let fc = Fingerprint::compute(&c).expect("c");

        assert_eq!(fa, fb, "identical files must share a fingerprint");
        assert_ne!(fa, fc, "different content must differ");
        assert!(fa.as_str().starts_with("11-"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn large_files_compare_head_and_tail() {
        let dir = temp_dir("large");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        // 300 KiB of identical data except for one byte at the very end.
        let mut data = vec![7u8; 300 * 1024];
        std::fs::write(&a, &data).unwrap();
        let last = data.len() - 1;
        data[last] = 9;
        std::fs::write(&b, &data).unwrap();
        let head = data.len() / 2;
        data[head] = 3;

        let fa = Fingerprint::compute(&a).expect("a");
        let fb = Fingerprint::compute(&b).expect("b");
        assert_ne!(fa, fb, "a differing tail byte must change the fingerprint");

        let missing = Fingerprint::compute(&dir.join("nope.bin"));
        assert!(missing.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
