use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use domain::ids::TrackId;
use domain::models::Track;
use eframe::egui::{self, TextureHandle};
use library::ArtworkManager;

/// Maximum edge length (px) of a texture uploaded to the GPU. Covers are
/// downscaled to this before upload so video/CPU memory stays tiny even for a
/// 10 000-track library.
const UPLOAD_MAX_EDGE: u32 = 160;

/// Maximum number of resident textures. Least-recently-used entries are evicted
/// on overflow, so steady-state texture memory tracks the visible viewport, not
/// the library size.
const MAX_RESIDENT_TEXTURES: usize = 256;

/// Service managing album art extraction, caching, and egui TextureHandle
/// generation.
///
/// Design goals (10k tracks on a dual-core / HDD machine):
/// * Never decode art for rows that are not on screen.
/// * Downscale to [`UPLOAD_MAX_EDGE`] before building the GPU texture.
/// * Bound resident textures with an LRU so scrolling can't grow memory.
/// * Remember covers that failed to load so we don't retry them every frame.
pub struct ArtworkService {
    manager: Arc<ArtworkManager>,
    texture_cache: HashMap<TrackId, TextureHandle>,
    /// LRU order: back = most recently used.
    order: VecDeque<TrackId>,
    /// Tracks whose art could not be resolved (avoids per-frame retries).
    missing: HashSet<TrackId>,
}

impl Default for ArtworkService {
    fn default() -> Self {
        Self::new()
    }
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
            order: VecDeque::new(),
            missing: HashSet::new(),
        }
    }

    pub fn manager(&self) -> Arc<ArtworkManager> {
        self.manager.clone()
    }

    /// Number of resident textures (for diagnostics / telemetry).
    pub fn resident_count(&self) -> usize {
        self.texture_cache.len()
    }

    pub fn get_texture(&self, track_id: &TrackId) -> Option<&TextureHandle> {
        self.texture_cache.get(track_id)
    }

    /// Read-only view of the texture cache (used where on-demand loading is not
    /// worth the extra plumbing, e.g. the Now Playing card).
    pub fn texture_cache(&self) -> &HashMap<TrackId, TextureHandle> {
        &self.texture_cache
    }

    pub fn is_missing(&self, track_id: &TrackId) -> bool {
        self.missing.contains(track_id)
    }

    /// Check if cover art exists for a track.
    pub fn has_artwork(&self, track_id: &TrackId, song_path: &std::path::Path) -> bool {
        self.manager.has_artwork(track_id, song_path)
    }

    /// Forget a failed lookup (e.g. after a new import or scan).
    pub fn invalidate(&mut self, track_id: &TrackId) {
        self.missing.remove(track_id);
    }

    /// Completely evict and invalidate a track's cached artwork.
    pub fn invalidate_track(&mut self, track_id: &TrackId) {
        self.missing.remove(track_id);
        self.texture_cache.remove(track_id);
        if let Some(pos) = self.order.iter().position(|id| id == track_id) {
            self.order.remove(pos);
        }
    }

    /// Clear all "missing" markers, e.g. after a library scan/import.
    pub fn clear_missing(&mut self) {
        self.missing.clear();
    }

    /// Mark a track as most-recently-used so it survives LRU eviction.
    pub fn touch(&mut self, track_id: &TrackId) {
        if let Some(pos) = self.order.iter().position(|id| id == track_id) {
            self.order.remove(pos);
            self.order.push_back(*track_id);
        }
    }

    /// Ensure an artwork texture is loaded for the given track. Cheap no-op when
    /// already resident or known-missing. Returns true if a texture is available.
    pub fn ensure_loaded(&mut self, ctx: &egui::Context, track: &Track) -> bool {
        self.ensure_loaded_at(ctx, track.id, &track.path)
    }

    /// Ensure an artwork texture is loaded for a track identified by its id and
    /// file path.
    ///
    /// The album grid knows an album's cover by the *track* it came from, and
    /// that is an id and a path — cloning whole `Track`s for every album in the
    /// library would be wasteful.
    pub fn ensure_loaded_at(
        &mut self,
        ctx: &egui::Context,
        id: TrackId,
        path: &std::path::Path,
    ) -> bool {
        if self.texture_cache.contains_key(&id) {
            self.touch(&id);
            return true;
        }
        if self.missing.contains(&id) {
            return false;
        }

        let cached_thumb = self.manager.get_cached_thumbnail_path(&id);
        let candidate = if cached_thumb.exists() {
            Some(cached_thumb)
        } else {
            self.manager.find_folder_artwork(path).or_else(|| {
                path.extension().and_then(|_| {
                    let png = path.with_extension("png");
                    if png.exists() {
                        return Some(png);
                    }
                    let jpg = path.with_extension("jpg");
                    if jpg.exists() {
                        return Some(jpg);
                    }
                    None
                })
            })
        };

        let Some(art_file) = candidate else {
            self.missing.insert(id);
            return false;
        };

        // Guard against oversized images stalling the UI thread or consuming excessive RAM
        if std::fs::metadata(&art_file)
            .map(|m| m.len() > 15 * 1024 * 1024)
            .unwrap_or(true)
        {
            self.missing.insert(id);
            return false;
        }

        let mut reader = match image::ImageReader::open(&art_file) {
            Ok(r) => match r.with_guessed_format() {
                Ok(guessed) => guessed,
                Err(_) => {
                    self.missing.insert(id);
                    return false;
                }
            },
            Err(_) => {
                self.missing.insert(id);
                return false;
            }
        };

        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);

        let Ok(img) = reader.decode() else {
            self.missing.insert(id);
            return false;
        };

        // Downscale before touching the GPU. `thumbnail` is a fast, allocation
        // friendly box filter, ideal for cover art.
        let img = if img.width() > UPLOAD_MAX_EDGE || img.height() > UPLOAD_MAX_EDGE {
            img.thumbnail(UPLOAD_MAX_EDGE, UPLOAD_MAX_EDGE)
        } else {
            img
        };

        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        let texture = ctx.load_texture(
            format!("art_{id}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );

        self.insert(id, texture);
        true
    }

    fn insert(&mut self, id: TrackId, texture: TextureHandle) {
        self.texture_cache.insert(id, texture);
        if let Some(pos) = self.order.iter().position(|x| *x == id) {
            self.order.remove(pos);
        }
        self.order.push_back(id);

        while self.order.len() > MAX_RESIDENT_TEXTURES {
            if let Some(oldest) = self.order.pop_front() {
                self.texture_cache.remove(&oldest);
            }
        }
    }
}
