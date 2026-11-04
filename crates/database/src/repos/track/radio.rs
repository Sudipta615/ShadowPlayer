use super::TrackRepo;
use crate::repos::common::map_track;
use domain::ids::*;
use domain::models::Track;
use domain::smart::RadioQuery;
use rusqlite::{params, Result};

impl TrackRepo {
    /// Build a local radio station around `seed`.
    ///
    /// The SQL prefilter keeps this affordable on a large library: only tracks
    /// that share a genre, artist or a tempo within 20 BPM are scored. If that
    /// pool is too small (a seed with no genre and no BPM), the rest of the
    /// library is considered rather than returning an almost-empty station.
    pub fn radio_station(&self, seed_id: &TrackId, limit: usize, salt: u64) -> Result<Vec<Track>> {
        let seed = match self.get_by_id(seed_id)? {
            Some(track) => track,
            None => return Ok(Vec::new()),
        };
        let query = RadioQuery {
            genre: seed.genre.clone(),
            artist: seed.artist.clone(),
            bpm: seed.bpm,
            rating: seed.rating,
            salt,
        };
        let limit = limit.clamp(1, 500);

        let mut pool: Vec<Track> = self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks
                  WHERE id != ?1
                    AND (genre = ?2 COLLATE NOCASE
                         OR artist = ?3 COLLATE NOCASE
                         OR (bpm IS NOT NULL AND ?4 IS NOT NULL
                             AND bpm BETWEEN ?4 - 20 AND ?4 + 20))
                  LIMIT 2000;",
            )?;
            let rows = stmt.query_map(
                params![seed.id.to_string(), seed.genre, seed.artist, seed.bpm],
                map_track,
            )?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })?;

        if pool.len() < limit {
            // Widen the net. This is still the user's own library and still
            // local; it just means the seed was too unusual to fill a station.
            let existing: std::collections::HashSet<TrackId> = pool.iter().map(|t| t.id).collect();
            let rest: Vec<Track> = self.db.with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT * FROM tracks WHERE id != ?1
                      ORDER BY play_count ASC, added_at DESC LIMIT 2000;",
                )?;
                let rows = stmt.query_map(params![seed.id.to_string()], map_track)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })?;
            pool.extend(rest.into_iter().filter(|t| !existing.contains(&t.id)));
        }

        pool.sort_by(|a, b| {
            query
                .score(b)
                .cmp(&query.score(a))
                .then_with(|| query.shuffle_key(a).cmp(&query.shuffle_key(b)))
        });
        pool.truncate(limit);
        Ok(pool)
    }
}
