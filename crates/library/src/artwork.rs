use domain::ids::TrackId;
use image::ImageReader;

/// Longest edge of a cached thumbnail, in pixels.
///
/// 160px is plenty for the 36px list rows and the 140px grid cards, and keeps
/// both the on-disk cache and the GPU texture memory small.
const THUMBNAIL_MAX_EDGE: u32 = 160;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub struct ArtworkManager {
    cache_dir: PathBuf,
}

impl ArtworkManager {
    pub fn new(cache_dir: impl AsRef<Path>) -> Self {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        let _ = fs::create_dir_all(&cache_dir);
        Self { cache_dir }
    }

    pub fn get_cached_thumbnail_path(&self, track_id: &TrackId) -> PathBuf {
        self.cache_dir.join(format!("{}.png", track_id))
    }

    /// Save image bytes as a bounded thumbnail (e.g. 300x300 max).
    pub fn save_thumbnail(&self, track_id: &TrackId, image_bytes: &[u8]) -> Option<PathBuf> {
        let reader = ImageReader::new(Cursor::new(image_bytes))
            .with_guessed_format()
            .ok()?;
        let img = reader.decode().ok()?;

        // `thumbnail` is a fast box filter; Lanczos3 would cost far more CPU
        // for no visible gain at this size.
        let thumb = img.thumbnail(THUMBNAIL_MAX_EDGE, THUMBNAIL_MAX_EDGE);
        let out_path = self.get_cached_thumbnail_path(track_id);

        if let Ok(mut file) = fs::File::create(&out_path) {
            if thumb.write_to(&mut file, image::ImageFormat::Png).is_ok() {
                return Some(out_path);
            }
        }

        None
    }

    /// Look for folder cover art beside audio file (e.g. cover.jpg, folder.png).
    pub fn find_folder_artwork(&self, song_path: &Path) -> Option<PathBuf> {
        let parent = song_path.parent()?;
        let candidates = [
            "cover.jpg",
            "cover.png",
            "cover.jpeg",
            "folder.jpg",
            "folder.png",
            "front.jpg",
            "front.png",
            "album.jpg",
            "album.png",
        ];

        for name in candidates {
            let p = parent.join(name);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Check if cover art exists for a track (cached thumbnail or folder image).
    pub fn has_artwork(&self, track_id: &TrackId, song_path: &Path) -> bool {
        self.get_cached_thumbnail_path(track_id).exists()
            || self.find_folder_artwork(song_path).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sp_artwork_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn png_of(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 0x80])
        });
        let mut bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode png");
        bytes
    }

    #[test]
    fn thumbnails_are_bounded_and_keep_their_aspect_ratio() {
        let dir = temp_dir("thumb");
        let manager = ArtworkManager::new(&dir);
        let id = TrackId::new();

        let wide = png_of(400, 100);
        let path = manager
            .save_thumbnail(&id, &wide)
            .expect("a valid PNG produces a thumbnail");
        assert_eq!(path, manager.get_cached_thumbnail_path(&id));

        let thumb = image::open(&path).expect("the thumbnail decodes");
        assert!(thumb.width() <= 160 && thumb.height() <= 160, "bounded");
        assert_eq!(thumb.width(), 160);
        assert_eq!(thumb.height(), 40, "aspect ratio is preserved");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bytes_that_are_not_an_image_are_refused() {
        let dir = temp_dir("garbage");
        let manager = ArtworkManager::new(&dir);
        let id = TrackId::new();

        assert!(manager
            .save_thumbnail(&id, b"this is not a picture")
            .is_none());
        assert!(!manager.get_cached_thumbnail_path(&id).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_artwork_is_found_beside_the_song() {
        let dir = temp_dir("folder");
        let manager = ArtworkManager::new(dir.join("cache"));
        let album = dir.join("album");
        std::fs::create_dir_all(&album).unwrap();
        let song = album.join("01 song.flac");

        assert!(manager.find_folder_artwork(&song).is_none());
        std::fs::write(album.join("cover.jpg"), b"jpeg-ish").unwrap();
        assert_eq!(
            manager.find_folder_artwork(&song),
            Some(album.join("cover.jpg"))
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
