use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
define_id!(ArtworkId);
define_id!(LyricsId);

static PATH_NAMESPACE: Uuid = Uuid::from_u128(0x716a5b6f_2c60_4387_9bc2_c77a672d6e3f);

impl TrackId {
    pub fn from_canonical_path(path: &std::path::Path) -> Self {
        let path_str = path.to_string_lossy();
        Self::from_name_and_seed(&PATH_NAMESPACE, &path_str)
    }
}
