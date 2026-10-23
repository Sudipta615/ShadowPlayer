//! Artist and album biographies, with a local cache and an optional folder of
//! user-supplied text files.
//!
//! There is no network lookup: a biography is either something the listener
//! wrote, or a `.txt`/`.md` file they put in a folder they chose. The cache in
//! the library database means the chosen source is read once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use database::BiographyRepo;
use domain::models::Biography;

pub struct BiographyService {
    repo: BiographyRepo,
    source_dir: Option<PathBuf>,
    cache: HashMap<(String, String), Option<Biography>>,
}

impl BiographyService {
    pub fn new(db: database::Database) -> Self {
        Self {
            repo: BiographyRepo::new(db),
            source_dir: None,
            cache: HashMap::new(),
        }
    }

    /// Point the service at a folder of `<artist>.txt` / `<album>.txt` files.
    /// An empty path clears the source.
    pub fn set_source_dir(&mut self, dir: &str) {
        let next = (!dir.trim().is_empty()).then(|| PathBuf::from(dir.trim()));
        if next != self.source_dir {
            self.source_dir = next;
            self.cache.clear();
        }
    }

    pub fn source_dir(&self) -> Option<&Path> {
        self.source_dir.as_deref()
    }

    /// The album cache key, which has to distinguish two albums with one title.
    pub fn album_key(artist: &str, title: &str) -> String {
        format!("{artist}\u{1f}{title}")
    }

    /// Look up a biography. The DB cache wins; a configured source folder is
    /// consulted only when the cache has nothing.
    pub fn get(&mut self, kind: &str, name: &str) -> Option<Biography> {
        let key = (kind.to_string(), name.to_string());
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }

        let found = match self.repo.get(kind, name) {
            Ok(Some(bio)) if !bio.text.trim().is_empty() => Some(bio),
            Ok(_) => self.read_source(kind, name),
            Err(e) => {
                log::warn!("Could not read the biography cache: {e}");
                self.read_source(kind, name)
            }
        };
        self.cache.insert(key, found.clone());
        found
    }

    /// Store a biography the listener wrote or pasted.
    pub fn save(&mut self, kind: &str, name: &str, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            self.repo.delete(kind, name).map_err(|e| {
                tr_args!(
                    "Could not remove the biography: {error}",
                    "error" => e
                )
            })?;
            self.cache
                .insert((kind.to_string(), name.to_string()), None);
            return Ok(());
        }
        self.repo.put(kind, name, text, "user").map_err(|e| {
            tr_args!(
                "Could not save the biography: {error}",
                "error" => e
            )
        })?;
        self.cache.insert(
            (kind.to_string(), name.to_string()),
            Some(Biography {
                entity_kind: kind.to_string(),
                entity_name: name.to_string(),
                text: text.to_string(),
                source: "user".to_string(),
                updated_at: 0,
            }),
        );
        Ok(())
    }

    fn read_source(&self, kind: &str, name: &str) -> Option<Biography> {
        let dir = self.source_dir.as_ref()?;

        // The album key uses a unit separator; a filename uses " - ".
        let (primary, alternatives): (String, Vec<String>) = if kind == "album" {
            let (artist, title) = name.split_once('\u{1f}').unwrap_or(("", name));
            (
                format!("{artist} - {title}"),
                vec![title.to_string(), format!("{title} - {artist}")],
            )
        } else {
            (name.to_string(), Vec::new())
        };

        let mut candidates = vec![primary];
        candidates.extend(alternatives);
        for stem in candidates {
            let safe = sanitize_file_stem(&stem);
            for extension in ["txt", "md", "markdown"] {
                let path = dir.join(format!("{safe}.{extension}"));
                if let Ok(text) = std::fs::read_to_string(&path) {
                    let text = text.trim();
                    if !text.is_empty() {
                        // Seed the cache so the next launch does not re-read
                        // the file, but keep the source name so the UI can say
                        // where the words came from.
                        let source = format!("local-file: {}", path.display());
                        let _ = self.repo.put(kind, name, text, &source);
                        return Some(Biography {
                            entity_kind: kind.to_string(),
                            entity_name: name.to_string(),
                            text: text.to_string(),
                            source,
                            updated_at: 0,
                        });
                    }
                }
            }
        }
        None
    }
}

/// Replace the characters that cannot appear in a filename on the platforms
/// the player ships on, while keeping spaces and Unicode letters.
fn sanitize_file_stem(stem: &str) -> String {
    stem.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            '\u{1f}' => '-',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_files_are_found_and_cached() {
        let dir = std::env::temp_dir().join(format!("sp_bio_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Boards of Canada.txt"), "A Scottish duo.").unwrap();

        let db = database::Database::in_memory().unwrap();
        let mut service = BiographyService::new(db);
        service.set_source_dir(dir.to_str().unwrap());

        let bio = service.get("artist", "Boards of Canada").unwrap();
        assert_eq!(bio.text, "A Scottish duo.");
        assert!(bio.source.starts_with("local-file"));
        // The DB cache now answers even after the source is removed.
        service.set_source_dir("");
        service.set_source_dir(dir.to_str().unwrap());
        std::fs::remove_file(dir.join("Boards of Canada.txt")).unwrap();
        assert_eq!(
            service.get("artist", "Boards of Canada").unwrap().text,
            "A Scottish duo."
        );
    }

    #[test]
    fn album_names_are_matched_with_the_artist_prefix() {
        let dir = std::env::temp_dir().join(format!("sp_bio_album_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Autechre - Amber.md"),
            "Amber is the second studio album.",
        )
        .unwrap();

        let db = database::Database::in_memory().unwrap();
        let mut service = BiographyService::new(db);
        service.set_source_dir(dir.to_str().unwrap());
        let key = BiographyService::album_key("Autechre", "Amber");
        let bio = service.get("album", &key).unwrap();
        assert!(bio.text.starts_with("Amber is"));
    }

    #[test]
    fn user_text_wins_over_the_source_folder() {
        let db = database::Database::in_memory().unwrap();
        let mut service = BiographyService::new(db);
        let key = BiographyService::album_key("Artist", "Album");
        service.save("album", &key, "My own notes").unwrap();
        assert_eq!(service.get("album", &key).unwrap().text, "My own notes");
    }
}
