use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[repr(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn from_name_and_seed(namespace: &Uuid, name: &str) -> Self {
                Self(Uuid::new_v5(namespace, name.as_bytes()))
            }

            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }

        impl From<Uuid> for $name {
            fn from(u: Uuid) -> Self {
                Self(u)
            }
        }
    };
}

define_id!(TrackId);
define_id!(AlbumId);
define_id!(ArtistId);
define_id!(PlaylistId);
define_id!(FolderId);
define_id!(ScanRootId);
define_id!(PodcastFeedId);
define_id!(PodcastEpisodeId);
define_id!(ZoneId);

static PATH_NAMESPACE: Uuid = Uuid::from_u128(0x716a5b6f_2c60_4387_9bc2_c77a672d6e3f);
static ALBUM_NAMESPACE: Uuid = Uuid::from_u128(0x3f0f7c2a_9d4b_4e21_8a55_c1e0b7ad2f19);
static ARTIST_NAMESPACE: Uuid = Uuid::from_u128(0x9b2d5e14_6c73_4a08_b2d9_07f4e1c8a630);

impl TrackId {
    pub fn from_canonical_path(path: &std::path::Path) -> Self {
        // The key is lossless (see `crate::paths`): two files whose names
        // differ only in bytes that are not valid UTF-8 used to lose those
        // bytes here and ended up with the same id.
        let path_key = crate::paths::encode_path_key(path);
        Self::from_name_and_seed(&PATH_NAMESPACE, &path_key)
    }
}

impl AlbumId {
    /// Stable id derived from the album's identity (title + artist), so that
    /// repeated lookups produce the same id. Selection and caches rely on this.
    pub fn from_identity(title: &str, artist: &str) -> Self {
        Self::from_name_and_seed(&ALBUM_NAMESPACE, &format!("{artist}\u{1f}{title}"))
    }
}

impl ArtistId {
    /// Stable id derived from the artist name.
    pub fn from_name(name: &str) -> Self {
        Self::from_name_and_seed(&ARTIST_NAMESPACE, name)
    }
}
