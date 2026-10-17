use domain::ids::TrackId;
use image::imageops::FilterType;
use image::ImageReader;
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

        // Resize down to 300x300 maintaining aspect ratio
        let thumb = img.resize(300, 300, FilterType::Lanczos3);
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
}
