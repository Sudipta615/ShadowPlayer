use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, TextureHandle};
use domain::ids::TrackId;
use domain::models::Track;
use library::ArtworkManager;

/// Service managing album art extraction, caching, and egui TextureHandle generation
pub struct ArtworkService {
    manager: Arc<ArtworkManager>,
    texture_cache: HashMap<TrackId, TextureHandle>,
}

impl ArtworkService {
    pub fn new() -> Self {
        let cache_dir = dirs::data_local_dir()
            .map(|d| d.join("shadowplayer").join("artwork_cache"))
            .unwrap_or_else(|| PathBuf::from("./data/artwork_cache"));
        let manager = Arc::new(ArtworkManager::new(&cache_dir));
        Self {
            manager,
            texture_cache: HashMap::new(),
        }
    }

    pub fn manager(&self) -> Arc<ArtworkManager> {
        self.manager.clone()
    }

    pub fn get_texture(&self, track_id: &TrackId) -> Option<&TextureHandle> {
        self.texture_cache.get(track_id)
    }

    pub fn texture_cache(&self) -> &HashMap<TrackId, TextureHandle> {
        &self.texture_cache
    }

    /// Ensure an artwork texture is loaded for the given track
    pub fn ensure_loaded(&mut self, ctx: &egui::Context, track: &Track) {
        if self.texture_cache.contains_key(&track.id) {
            return;
        }

        let cached_thumb = self.manager.get_cached_thumbnail_path(&track.id);
        let png_path = track.path.with_extension("png");
        let jpg_path = track.path.with_extension("jpg");
        let cover_path = track.path.parent().map(|p| p.join("cover.png")).unwrap_or_default();
        let cover_jpg = track.path.parent().map(|p| p.join("cover.jpg")).unwrap_or_default();

        let candidate = if cached_thumb.exists() {
            Some(cached_thumb)
        } else if png_path.exists() {
            Some(png_path)
        } else if jpg_path.exists() {
            Some(jpg_path)
        } else if cover_path.exists() {
            Some(cover_path)
        } else if cover_jpg.exists() {
            Some(cover_jpg)
        } else {
            None
        };

        if let Some(art_file) = candidate {
            if let Ok(img) = image::open(&art_file) {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                let texture = ctx.load_texture(
                    format!("art_{}", track.id),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                self.texture_cache.insert(track.id, texture);
            }
        }
    }
}
