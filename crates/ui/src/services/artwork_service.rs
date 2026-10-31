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

/// Maximum number of blurred backdrops kept on the GPU. Only the current track
/// needs one, plus a little history for quick A/B track changes.
const MAX_BACKDROPS: usize = 4;

/// The side of the tiny source image kept for the frosted backdrop. Blurring a
/// 64×64 image is effectively free and yields a beautifully soft colour field.
const BACKDROP_SOURCE_EDGE: u32 = 64;

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
    /// Tiny colour sources used to build the frosted-glass backdrop. These are
    /// prepared while the artwork is decoded, so the backdrop costs no extra
    /// disk I/O or decode later.
    backdrop_sources: HashMap<TrackId, egui::ColorImage>,
    /// Blurred GPU copies, generated on demand for the current track.
    backdrop_textures: HashMap<TrackId, TextureHandle>,
    backdrop_order: VecDeque<TrackId>,
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
            backdrop_sources: HashMap::new(),
            backdrop_textures: HashMap::new(),
            backdrop_order: VecDeque::new(),
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

    /// The blurred backdrop for this track, generated once from the tiny
    /// colour source captured at artwork-load time.
    ///
    /// Returns a clone of the texture handle so callers can paint while the
    /// service continues to be borrowed mutably for other work.
    pub fn backdrop_texture(
        &mut self,
        ctx: &egui::Context,
        track_id: &TrackId,
    ) -> Option<TextureHandle> {
        if let Some(texture) = self.backdrop_textures.get(track_id) {
            if let Some(pos) = self.backdrop_order.iter().position(|id| id == track_id) {
                self.backdrop_order.remove(pos);
            }
            self.backdrop_order.push_back(*track_id);
            return Some(texture.clone());
        }

        let source = self.backdrop_sources.get(track_id)?;
        let blurred = blur_color_image(source, 4);
        let texture = ctx.load_texture(
            format!("art_backdrop_{track_id}"),
            blurred,
            egui::TextureOptions::LINEAR,
        );
        self.backdrop_textures.insert(*track_id, texture.clone());
        self.backdrop_order.push_back(*track_id);
        while self.backdrop_order.len() > MAX_BACKDROPS {
            if let Some(oldest) = self.backdrop_order.pop_front() {
                self.backdrop_textures.remove(&oldest);
            }
        }
        Some(texture)
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
        self.backdrop_sources.remove(track_id);
        self.backdrop_textures.remove(track_id);
        if let Some(pos) = self.order.iter().position(|id| id == track_id) {
            self.order.remove(pos);
        }
        if let Some(pos) = self.backdrop_order.iter().position(|id| id == track_id) {
            self.backdrop_order.remove(pos);
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

        // Keep a tiny colour source for the frosted-glass backdrop. Doing this
        // now costs one fast thumbnail of an image we have already decoded, and
        // avoids a second disk read when the track starts playing.
        let tiny = img.thumbnail(BACKDROP_SOURCE_EDGE, BACKDROP_SOURCE_EDGE);
        let tiny = tiny.to_rgba8();
        let source = egui::ColorImage::from_rgba_unmultiplied(
            [tiny.width() as usize, tiny.height() as usize],
            tiny.as_raw(),
        );
        self.backdrop_sources.insert(id, source);
        // The source changed, so any previous blurred copy is stale.
        self.backdrop_textures.remove(&id);
        if let Some(pos) = self.backdrop_order.iter().position(|entry| entry == &id) {
            self.backdrop_order.remove(pos);
        }

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
                self.backdrop_sources.remove(&oldest);
                self.backdrop_textures.remove(&oldest);
                if let Some(pos) = self.backdrop_order.iter().position(|id| id == &oldest) {
                    self.backdrop_order.remove(pos);
                }
            }
        }
    }
}

/// Separable box blur over a tiny RGBA image. Two passes are enough to erase
/// album-art detail while keeping the palette and light distribution, which is
/// exactly what a frosted pane transmits.
fn blur_color_image(source: &egui::ColorImage, radius: usize) -> egui::ColorImage {
    let [w, h] = source.size;
    if w == 0 || h == 0 || radius == 0 {
        return source.clone();
    }
    let mut horizontal = vec![egui::Color32::TRANSPARENT; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut a = 0u32;
            let mut n = 0u32;
            let lo = x.saturating_sub(radius);
            let hi = (x + radius).min(w - 1);
            for sx in lo..=hi {
                let c = source.pixels[y * w + sx];
                r += c.r() as u32;
                g += c.g() as u32;
                b += c.b() as u32;
                a += c.a() as u32;
                n += 1;
            }
            horizontal[y * w + x] = crate::theme::rgba_unmultiplied(
                (r / n) as u8,
                (g / n) as u8,
                (b / n) as u8,
                (a / n) as u8,
            );
        }
    }

    let mut out = vec![egui::Color32::TRANSPARENT; w * h];
    for y in 0..h {
        let lo = y.saturating_sub(radius);
        let hi = (y + radius).min(h - 1);
        for x in 0..w {
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut a = 0u32;
            let mut n = 0u32;
            for sy in lo..=hi {
                let c = horizontal[sy * w + x];
                r += c.r() as u32;
                g += c.g() as u32;
                b += c.b() as u32;
                a += c.a() as u32;
                n += 1;
            }
            out[y * w + x] = crate::theme::rgba_unmultiplied(
                (r / n) as u8,
                (g / n) as u8,
                (b / n) as u8,
                (a / n) as u8,
            );
        }
    }

    egui::ColorImage::new([w, h], out)
}
