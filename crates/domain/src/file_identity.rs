//! The operating system's identity for a file.
//!
//! A path is a *name*. Two names can point at one file, and one name can
//! point at different files over time. What identifies a file, on every
//! platform, is what the filesystem itself says about it: the device it lives
//! on and the index of the entry within that device.
//!
//! This matters because a case-insensitive filesystem — which is the default
//! on Windows and on macOS — makes `Track.FLAC` and `track.flac` the *same
//! file* with two spellings. A library that keys on the spelling stores the
//! file twice: the row for the old spelling is never pruned (the path still
//! resolves), and the new spelling becomes a second track with its own
//! play count, its own favourites and its own place in every playlist.
//!
//! Keying on the filesystem's own identity fixes that, and fixes a family of
//! neighbours with it: a file that is renamed, moved, or moved onto another
//! volume and back, keeps its identity because the file never changed — only
//! its name did.
//!
//! What it deliberately does **not** do is merge two files that merely look
//! alike. A copy has a different identity, even when its bytes are identical,
//! and it is indexed as a separate track. That is the right answer: they are
//! two files, and the user may want both.
//!
//! The one caveat is hard links. Two paths for one inode are one file by the
//! filesystem's own reckoning, so they resolve to one track. That is honest
//! rather than convenient, and it is what the platform itself believes.

use std::path::Path;

/// The filesystem's identity for one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    /// The volume the file lives on.
    pub device: u64,
    /// The file's index within that volume.
    pub inode: u64,
}

impl FileIdentity {
    /// The stable text form stored in the database.
    ///
    /// A plain `device:inode` pair: it is short, it sorts, and it never has to
    /// be parsed back into anything but the same two numbers.
    pub fn to_key(self) -> String {
        format!("{}:{}", self.device, self.inode)
    }

    /// Read a key back.
    pub fn from_key(key: &str) -> Option<Self> {
        let (device, inode) = key.split_once(':')?;
        Some(FileIdentity {
            device: device.parse().ok()?,
            inode: inode.parse().ok()?,
        })
    }
}

/// The identity of the file at `path`.
///
/// `None` when the file cannot be reached — it was deleted between the walk
/// and the call, or the platform refused to answer. Callers treat that as
/// "identity unknown", never as "identity zero".
pub fn file_identity(path: &Path) -> Option<FileIdentity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path).ok()?;
        Some(FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    #[cfg(windows)]
    {
        // The standard library's `MetadataExt` exposes the volume serial
        // number and the file index, but both are still unstable. This asks
        // the filesystem directly, which is the same answer NTFS itself
        // uses to name a file — and it is stable across a rename and across
        // a case-only rename.
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };

        let file = std::fs::File::open(path).ok()?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the handle comes from a live `File`, and the output struct
        // is a plain fixed-size POD.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }.ok()?;
        Some(FileIdentity {
            device: info.dwVolumeSerialNumber as u64,
            inode: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        })
    }

    #[cfg(not(any(unix, windows)))]
    {
        // No platform API to ask. The identity is unknown rather than wrong.
        None
    }
}

/// True when the directory still holds an entry with exactly this name.
///
/// This is what separates a *rename* from a *hard link*.
///
/// A rename replaces the directory entry, so the old spelling disappears from
/// the listing — even on a case-insensitive filesystem, where the old path
/// still resolves to the file. A hard link adds a second entry, so the old
/// spelling is still there, and the two paths are genuinely two entries in the
/// library that happen to be one file.
///
/// The distinction matters: a rename should move the track's row, and a hard
/// link should leave both rows alone.
pub fn directory_still_holds(parent: &Path, file_name: &std::ffi::OsStr) -> bool {
    let Ok(entries) = std::fs::read_dir(parent) else {
        // The directory cannot be read, so the question cannot be answered.
        // Treating it as "still there" keeps the row where it is, which is the
        // conservative choice: nothing is merged on a guess.
        return true;
    };
    for entry in entries.flatten() {
        if entry.file_name() == file_name {
            return true;
        }
    }
    false
}

/// True when the path no longer names an entry in its directory.
///
/// The inverse of [`directory_still_holds`], spelled out for the call sites
/// where that is what is being asked.
pub fn entry_has_gone(path: &Path) -> bool {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    !directory_still_holds(parent, name)
}

#[cfg(test)]
mod tests {
    use super::{directory_still_holds, entry_has_gone, file_identity, FileIdentity};
    use std::path::Path;

    #[test]
    fn a_key_round_trips() {
        let id = FileIdentity {
            device: 65_534,
            inode: 9_223_372_036_854_775_807,
        };
        let key = id.to_key();
        assert_eq!(FileIdentity::from_key(&key), Some(id));
    }

    #[test]
    fn a_broken_key_is_not_an_identity() {
        assert_eq!(FileIdentity::from_key(""), None);
        assert_eq!(FileIdentity::from_key("no-colon"), None);
        assert_eq!(FileIdentity::from_key("a:b"), None);
        assert_eq!(FileIdentity::from_key("1:"), None);
    }

    #[test]
    fn a_file_has_an_identity_and_a_missing_one_does_not() {
        let dir = std::env::temp_dir().join(format!("sp_fileid_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("track.flac");
        std::fs::write(&file, b"x").unwrap();

        let identity = file_identity(&file).expect("an existing file has an identity");
        assert_eq!(file_identity(&file), Some(identity));

        // The same file, reached by a different name, is the same file.
        let renamed = dir.join("renamed.flac");
        std::fs::rename(&file, &renamed).unwrap();
        assert_eq!(file_identity(&renamed), Some(identity));

        // A missing file has no identity — and never a fabricated one.
        assert_eq!(file_identity(&dir.join("absent.flac")), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn two_files_with_the_same_name_in_different_directories_are_different_files() {
        let root = std::env::temp_dir().join(format!("sp_fileid_two_{}", std::process::id()));
        let a_dir = root.join("a");
        let b_dir = root.join("b");
        std::fs::create_dir_all(&a_dir).unwrap();
        std::fs::create_dir_all(&b_dir).unwrap();
        let a = a_dir.join("track.flac");
        let b = b_dir.join("track.flac");
        std::fs::write(&a, b"x").unwrap();
        std::fs::write(&b, b"x").unwrap();

        let a_id = file_identity(&a).unwrap();
        let b_id = file_identity(&b).unwrap();
        assert_ne!(a_id, b_id, "different files, even with identical bytes");

        std::fs::remove_dir_all(&root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_is_a_different_file() {
        let root = std::env::temp_dir().join(format!("sp_fileid_copy_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("original.flac");
        let copy = root.join("copy.flac");
        std::fs::write(&original, b"x").unwrap();
        std::fs::copy(&original, &copy).unwrap();

        assert_ne!(
            file_identity(&original),
            file_identity(&copy),
            "a copy is a second file, not the first one again"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_directory_has_an_identity_too() {
        // The API is about files, but nothing stops it being asked about a
        // directory; the answer must be a real identity, not a panic.
        let dir = std::env::temp_dir();
        if dir.exists() {
            assert!(file_identity(Path::new(&dir)).is_some());
        }
    }

    #[test]
    fn a_rename_removes_the_old_entry_and_a_hard_link_keeps_it() {
        let root = std::env::temp_dir().join(format!("sp_entries_{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        let original = root.join("Track.FLAC");
        std::fs::write(&original, b"x").unwrap();
        assert!(directory_still_holds(
            &root,
            std::ffi::OsStr::new("Track.FLAC")
        ));
        assert!(!entry_has_gone(&original));

        // A rename replaces the entry: the old spelling is gone from the
        // directory even though the path may still resolve.
        let renamed = root.join("track.flac");
        std::fs::rename(&original, &renamed).unwrap();
        assert!(!directory_still_holds(
            &root,
            std::ffi::OsStr::new("Track.FLAC")
        ));
        assert!(entry_has_gone(&original));

        // A hard link adds a second entry: the old spelling is still there.
        let link = root.join("link.flac");
        std::fs::hard_link(&renamed, &link).unwrap();
        assert!(directory_still_holds(
            &root,
            std::ffi::OsStr::new("track.flac")
        ));
        assert!(!entry_has_gone(&renamed));

        std::fs::remove_dir_all(&root).ok();
    }
}
