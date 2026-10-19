//! Cheap, dependency-free content fingerprinting.
//!
//! The goal is *not* cryptographic integrity but a stable identity for a media
//! file that survives a move or rename: we combine the file size with a fast
//! FNV-1a hash of the first, middle and last 64 KiB. Three probes rather than
//! two is deliberate — head and tail alone let two different files that share
//! their opening and closing 64 KiB collide, and a collision here can hand one
//! track's play counts, favourites and playlist membership to another file.
//! It stays far cheaper than hashing a whole multi-hundred-megabyte FLAC.
//!
//! [`Fingerprint::compute`] returns `None` when the file cannot be read, in
//! which case callers keep whatever identity they had.

use std::fmt::Write as _;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How many bytes are sampled at each probe point.
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

        // Probe points: head, middle, tail. A file short enough to fit in one
        // sample is read once from the start.
        let mut offsets: Vec<u64> = if len <= SAMPLE_BYTES {
            vec![0]
        } else {
            let mut v = vec![0u64, (len - SAMPLE_BYTES) / 2, len - SAMPLE_BYTES];
            v.dedup();
            v
        };
        offsets.shrink_to_fit();

        let mut buf = vec![0u8; SAMPLE_BYTES.min(len) as usize];
        for offset in offsets {
            // A file that cannot be read all the way through gets no
            // fingerprint at all, so it can never be mistaken for another.
            file.seek(SeekFrom::Start(offset)).ok()?;
            file.read_exact(&mut buf).ok()?;
            mix(&buf);
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
    fn large_files_compare_head_middle_and_tail() {
        let dir = temp_dir("large");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        let c = dir.join("c.bin");

        // 300 KiB of identical data, then one byte changed at the very end
        // (`b`) and one in the middle only (`c`). The middle probe exists
        // precisely so `c` cannot collide with `a`.
        let data = vec![7u8; 300 * 1024];
        std::fs::write(&a, &data).unwrap();

        let mut tail_changed = data.clone();
        let last = tail_changed.len() - 1;
        tail_changed[last] = 9;
        std::fs::write(&b, &tail_changed).unwrap();

        let mut middle_changed = data.clone();
        let middle = middle_changed.len() / 2;
        middle_changed[middle] = 3;
        std::fs::write(&c, &middle_changed).unwrap();

        let fa = Fingerprint::compute(&a).expect("a");
        let fb = Fingerprint::compute(&b).expect("b");
        let fc = Fingerprint::compute(&c).expect("c");
        assert_ne!(fa, fb, "a differing tail byte must change the fingerprint");
        assert_ne!(
            fa, fc,
            "a differing middle byte must change the fingerprint"
        );
        assert_ne!(fb, fc);

        let missing = Fingerprint::compute(&dir.join("nope.bin"));
        assert!(missing.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
