use super::{
    deterministic_tie_break, recency_penalty, score_candidate, HeapEntry, ScoredCandidate,
};
use super::{Recommendation, RecommendationConfig, RecommendationRequest, Track};
use crate::{ArtistId, TasteProfile, TrackId};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

#[derive(Clone, Debug)]
pub struct Recommender {
    pub config: RecommendationConfig,
}

impl Default for Recommender {
    fn default() -> Self {
        Self::new(RecommendationConfig::default())
    }
}

impl Recommender {
    pub fn new(config: RecommendationConfig) -> Self {
        Self { config }
    }

    /// Score candidates and greedily diversify the best candidate pool using MMR.
    ///
    /// Complexity is `O(N × 20 + N log P + P log P + K × P × 20)` for `N`
    /// tracks, a bounded pool of `P` candidates, and `K` results: scoring is a
    /// single pass, the heap keeps only the best pool, and each selected track
    /// updates the running maximum similarity of the remaining pool. It does
    /// not decode audio, access the database, or spawn threads.
    pub fn recommend(
        &self,
        tracks: &[Track],
        profile: &TasteProfile,
        request: &RecommendationRequest<'_>,
    ) -> Vec<Recommendation> {
        if request.limit == 0 || tracks.is_empty() {
            return Vec::new();
        }
        let config = self.config.sanitized();
        let total_weight = config.total_weight();
        let result_limit = request.limit.min(tracks.len());
        let seed = request
            .seed_track_id
            .and_then(|seed_id| tracks.iter().find(|track| track.id == seed_id));
        let excluded: HashSet<TrackId> = request.excluded_track_ids.iter().copied().collect();
        let recent_tracks: HashSet<TrackId> = request.recently_played_ids.iter().copied().collect();
        let recent_artists: HashSet<ArtistId> = request.recent_artist_ids.iter().copied().collect();

        let scaled_pool = result_limit
            .saturating_mul(config.candidate_pool_factor)
            .max(result_limit);
        let pool_limit = scaled_pool
            .min(config.candidate_pool_max.max(result_limit))
            .min(tracks.len());
        // Keep only the best candidate pool in a bounded heap. A 100k-track
        // library therefore doesn't allocate 100k intermediate score objects.
        let mut heap = BinaryHeap::with_capacity(pool_limit.min(tracks.len()));
        for (order, track) in tracks.iter().enumerate() {
            if Some(track.id) == request.seed_track_id || excluded.contains(&track.id) {
                continue;
            }
            let in_recent_list = recent_tracks.contains(&track.id);
            let recent_artist = track
                .metadata
                .artist_id
                .is_some_and(|id| recent_artists.contains(&id));
            let (base_score, breakdown, reasons) =
                score_candidate(track, seed, profile, request.now_ms, &config, total_weight);
            let recency_penalty = recency_penalty(track, request.now_ms, in_recent_list, &config)
                + if recent_artist {
                    config.artist_repeat_penalty * 0.5
                } else {
                    0.0
                };
            let candidate = ScoredCandidate {
                track_id: track.id,
                artist_id: track.metadata.artist_id,
                features: track.features,
                base_score,
                priority: (base_score - recency_penalty).clamp(0.0, 1.0),
                tie_breaker: deterministic_tie_break(
                    track.id,
                    request.seed_track_id,
                    request.now_ms,
                ),
                max_similarity: 0.0,
                order,
                reasons,
                breakdown,
            };
            let entry = HeapEntry { candidate, order };
            if heap.len() < pool_limit {
                heap.push(entry);
            } else if heap.peek().is_some_and(|worst: &HeapEntry| {
                entry
                    .candidate
                    .priority
                    .total_cmp(&worst.candidate.priority)
                    .then_with(|| {
                        entry
                            .candidate
                            .tie_breaker
                            .total_cmp(&worst.candidate.tie_breaker)
                    })
                    == Ordering::Greater
            }) {
                heap.pop();
                heap.push(entry);
            }
        }

        let mut pool: Vec<ScoredCandidate> =
            heap.into_iter().map(|entry| entry.candidate).collect();
        pool.sort_unstable_by(|left, right| {
            right
                .priority
                .total_cmp(&left.priority)
                .then_with(|| right.tie_breaker.total_cmp(&left.tie_breaker))
                .then_with(|| left.order.cmp(&right.order))
        });

        let mut selected = vec![false; pool.len()];
        let mut selected_artists = HashSet::with_capacity(result_limit);
        let mut results = Vec::with_capacity(result_limit.min(pool.len()));
        while results.len() < result_limit {
            let mut best_index = None;
            let mut best_final_score = f32::NEG_INFINITY;
            for (index, candidate) in pool.iter().enumerate() {
                if selected[index] {
                    continue;
                }
                let repeats_artist = candidate
                    .artist_id
                    .is_some_and(|artist| selected_artists.contains(&artist));
                let diversity_penalty = config.diversity_lambda * candidate.max_similarity
                    + if repeats_artist {
                        config.artist_repeat_penalty
                    } else {
                        0.0
                    };
                let final_score = (candidate.priority - diversity_penalty).clamp(0.0, 1.0);
                if final_score > best_final_score {
                    best_final_score = final_score;
                    best_index = Some(index);
                }
            }
            let Some(best_index) = best_index else {
                break;
            };
            let chosen = pool[best_index];
            selected[best_index] = true;
            results.push(Recommendation {
                track_id: chosen.track_id,
                score: best_final_score,
                base_score: chosen.base_score,
                confidence: chosen.breakdown.coverage,
                reasons: chosen.reasons,
                breakdown: chosen.breakdown,
            });
            if let Some(artist) = chosen.artist_id {
                selected_artists.insert(artist);
            }
            // Update the running maximum similarity for the remaining pool.
            // This is the same MMR objective as recomputing against every
            // selected item, but avoids the O(K²) repeated scan.
            if let Some(chosen_features) = chosen.features {
                for (index, candidate) in pool.iter_mut().enumerate() {
                    if selected[index] {
                        continue;
                    }
                    if let Some(features) = candidate.features {
                        if let Some(similarity) = chosen_features.similarity(&features) {
                            if similarity > candidate.max_similarity {
                                candidate.max_similarity = similarity;
                            }
                        }
                    }
                }
            }
        }
        results
    }
}
