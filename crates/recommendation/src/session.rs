use crate::{
    ArtistId, FeedbackEvent, FeedbackKind, Recommendation, RecommendationConfig,
    RecommendationRequest, Recommender, TasteProfile, TasteProfileConfig, Track, TrackId,
};
use std::collections::{HashMap, HashSet};

/// Settings for a rolling recommendation session.
#[derive(Clone, Debug)]
pub struct RecommendationSessionConfig {
    /// Number of new tracks returned by each `start`/`refill` call.
    pub batch_size: usize,
    /// Seed track used for content similarity and transition context.
    pub seed_track_id: Option<TrackId>,
    /// Taste-profile settings.
    pub profile: TasteProfileConfig,
    /// Ranking settings.
    pub recommendation: RecommendationConfig,
}

impl Default for RecommendationSessionConfig {
    fn default() -> Self {
        Self {
            batch_size: 12,
            seed_track_id: None,
            profile: TasteProfileConfig::default(),
            recommendation: RecommendationConfig::default(),
        }
    }
}

/// Stateful, host-driven recommendation list.
///
/// `Recommender` is intentionally stateless: it ranks a catalog for one
/// request. This session keeps the rolling list, exclusions, playback order, and
/// feedback history needed for a Spotify/YouTube-style “For You” section.
///
/// The host controls the lifecycle:
///
/// 1. create a session from the library and existing feedback history;
/// 2. call [`start`](Self::start) to get the first batch;
/// 3. call [`mark_played`](Self::mark_played) and
///    [`record_feedback`](Self::record_feedback) as playback happens;
/// 4. call [`refill`](Self::refill) when the pending list gets short;
/// 5. display [`items`](Self::items) or only [`pending_items`](Self::pending_items).
///
/// `refill` rebuilds the taste profile from the accumulated events before
/// ranking, so new likes, skips, ratings, completions, and replays affect the
/// next batch. Skipped or disliked tracks are removed from the displayed list
/// immediately; already-played tracks stay in `items` for history but are never
/// returned again.
///
/// The session starts no threads and performs no I/O. Call it from the host's
/// worker/queue code, never from the real-time audio callback.
#[derive(Clone, Debug)]
pub struct RecommendationSession {
    config: RecommendationSessionConfig,
    profile: TasteProfile,
    events: Vec<FeedbackEvent>,
    recommender: Recommender,
    items: Vec<Recommendation>,
    recommended: HashSet<TrackId>,
    excluded: HashSet<TrackId>,
    played: Vec<TrackId>,
    played_set: HashSet<TrackId>,
    artist_by_track: HashMap<TrackId, ArtistId>,
    recent_artist_ids: Vec<ArtistId>,
}

impl RecommendationSession {
    /// Build a session and its initial taste profile.
    ///
    /// `history` should contain the feedback the host already has. New events
    /// are added with [`record_feedback`](Self::record_feedback).
    pub fn new(
        tracks: &[Track],
        history: Vec<FeedbackEvent>,
        now_ms: i64,
        config: RecommendationSessionConfig,
    ) -> Self {
        let profile = TasteProfile::build(tracks, &history, now_ms, &config.profile);
        let artist_by_track = tracks
            .iter()
            .filter_map(|track| track.metadata.artist_id.map(|artist| (track.id, artist)))
            .collect();
        Self {
            recommender: Recommender::new(config.recommendation.clone()),
            config,
            profile,
            events: history,
            items: Vec::new(),
            recommended: HashSet::new(),
            excluded: HashSet::new(),
            played: Vec::new(),
            played_set: HashSet::new(),
            artist_by_track,
            recent_artist_ids: Vec::new(),
        }
    }

    /// Return the current session configuration.
    pub fn config(&self) -> &RecommendationSessionConfig {
        &self.config
    }

    /// Replace the seed track. The next `refill` uses the new context.
    pub fn set_seed(&mut self, seed_track_id: Option<TrackId>) {
        self.config.seed_track_id = seed_track_id;
    }

    /// Change the batch size for future `refill` calls.
    pub fn set_batch_size(&mut self, batch_size: usize) {
        self.config.batch_size = batch_size.max(1);
    }

    /// Return the current taste profile.
    pub fn profile(&self) -> &TasteProfile {
        &self.profile
    }

    /// Return the feedback events accumulated by the session.
    pub fn events(&self) -> &[FeedbackEvent] {
        &self.events
    }

    /// Return every recommendation appended to the session, including played
    /// tracks. Use [`pending_items`](Self::pending_items) for the unplayed view.
    pub fn items(&self) -> &[Recommendation] {
        &self.items
    }

    /// Iterate over recommendations that have not been marked played.
    pub fn pending_items(&self) -> impl Iterator<Item = &Recommendation> + '_ {
        self.items
            .iter()
            .filter(|item| !self.played_set.contains(&item.track_id))
    }

    /// Number of recommendations still waiting to be played.
    pub fn pending_count(&self) -> usize {
        self.pending_items().count()
    }

    /// Return the first batch. This is equivalent to the first `refill`.
    pub fn start(&mut self, tracks: &[Track], now_ms: i64) -> Vec<Recommendation> {
        self.refill(tracks, now_ms)
    }

    /// Rebuild the profile from accumulated feedback and append a new batch.
    ///
    /// Previously returned tracks, played tracks, and explicitly excluded tracks
    /// are not returned again. The returned vector contains only the newly added
    /// items; `items()` contains the full rolling list.
    pub fn refill(&mut self, tracks: &[Track], now_ms: i64) -> Vec<Recommendation> {
        if self.config.batch_size == 0 || tracks.is_empty() {
            return Vec::new();
        }
        self.rebuild_profile(tracks, now_ms);

        let mut excluded = self.excluded.clone();
        excluded.extend(self.recommended.iter().copied());
        excluded.extend(self.played_set.iter().copied());
        let excluded_ids: Vec<TrackId> = excluded.into_iter().collect();
        let recent_ids: Vec<TrackId> = self.played.iter().rev().take(64).copied().collect();
        let request = RecommendationRequest {
            seed_track_id: self.config.seed_track_id,
            limit: self.config.batch_size,
            now_ms,
            excluded_track_ids: &excluded_ids,
            recently_played_ids: &recent_ids,
            recent_artist_ids: &self.recent_artist_ids,
        };
        let batch = self.recommender.recommend(tracks, &self.profile, &request);
        for item in &batch {
            self.recommended.insert(item.track_id);
        }
        self.items.extend(batch.iter().cloned());
        batch
    }

    /// Re-rank the unplayed part of the current list with the latest profile.
    ///
    /// `refill` already uses new feedback for newly appended tracks. Call this
    /// when the UI should also reorder the tracks the listener has not played
    /// yet. Played items stay in `items()` but are not re-ranked or returned.
    pub fn rerank_pending(&mut self, tracks: &[Track], now_ms: i64) -> Vec<Recommendation> {
        self.rebuild_profile(tracks, now_ms);
        let pending_ids: HashSet<TrackId> =
            self.pending_items().map(|item| item.track_id).collect();
        if pending_ids.is_empty() {
            return Vec::new();
        }
        let pending_tracks: Vec<Track> = tracks
            .iter()
            .filter(|track| pending_ids.contains(&track.id))
            .cloned()
            .collect();
        if pending_tracks.is_empty() {
            return Vec::new();
        }
        let recent_ids: Vec<TrackId> = self.played.iter().rev().take(64).copied().collect();
        let request = RecommendationRequest {
            seed_track_id: self.config.seed_track_id,
            limit: pending_tracks.len(),
            now_ms,
            excluded_track_ids: &[],
            recently_played_ids: &recent_ids,
            recent_artist_ids: &self.recent_artist_ids,
        };
        let reranked = self
            .recommender
            .recommend(&pending_tracks, &self.profile, &request);
        let mut items: Vec<Recommendation> = self
            .items
            .iter()
            .filter(|item| self.played_set.contains(&item.track_id))
            .cloned()
            .collect();
        items.extend(reranked.iter().cloned());
        self.items = items;
        reranked
    }

    /// Mark a track as played and exclude it from future batches.
    pub fn mark_played(&mut self, track_id: TrackId) {
        self.excluded.insert(track_id);
        if self.played_set.insert(track_id) {
            self.played.push(track_id);
            if let Some(artist) = self.artist_by_track.get(&track_id).copied() {
                if !self.recent_artist_ids.contains(&artist) {
                    self.recent_artist_ids.push(artist);
                    if self.recent_artist_ids.len() > 32 {
                        self.recent_artist_ids.remove(0);
                    }
                }
            }
        }
    }

    /// Remove a track from the displayed list and exclude it from future
    /// batches. Useful when the host has its own “not interested” action.
    pub fn remove(&mut self, track_id: TrackId) {
        self.excluded.insert(track_id);
        self.items.retain(|item| item.track_id != track_id);
    }

    /// Record feedback and update the session's view immediately.
    ///
    /// The profile is rebuilt lazily by the next `refill`, not here. Skipped and
    /// disliked tracks are removed from the displayed list right away.
    pub fn record_feedback(&mut self, event: FeedbackEvent) {
        let track_id = event.track_id;
        let kind = event.kind;
        let is_play = matches!(
            kind,
            FeedbackKind::PlayStarted
                | FeedbackKind::Completed
                | FeedbackKind::Skipped { .. }
                | FeedbackKind::Replay
        );
        let is_negative = matches!(kind, FeedbackKind::Disliked | FeedbackKind::Skipped { .. });

        self.events.push(event);
        self.excluded.insert(track_id);
        if is_play {
            self.mark_played(track_id);
        }
        if is_negative {
            self.items.retain(|item| item.track_id != track_id);
        }
    }

    /// Rebuild the taste profile without requesting new recommendations.
    pub fn rebuild_profile(&mut self, tracks: &[Track], now_ms: i64) {
        self.profile = TasteProfile::build(tracks, &self.events, now_ms, &self.config.profile);
    }

    /// Whether every track in the supplied catalog has already been recommended
    /// or excluded.
    pub fn is_exhausted(&self, tracks: &[Track]) -> bool {
        tracks
            .iter()
            .all(|track| self.recommended.contains(&track.id) || self.excluded.contains(&track.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeatureVector, TrackMetadata, TrackStats, FEATURE_COUNT};

    fn track(id: u128, artist: u128, marker: usize) -> Track {
        let mut values = [0.0_f32; FEATURE_COUNT];
        values[0] = marker as f32 / 10.0;
        values[1] = marker as f32 / 10.0;
        values[8 + marker % 12] = 1.0;
        Track {
            id,
            features: Some(FeatureVector::new(values, (1_u32 << FEATURE_COUNT) - 1)),
            metadata: TrackMetadata {
                artist_id: Some(artist),
                album_id: Some(id as u64),
                genre_ids: vec![id as u64 % 4 + 1],
                duration_seconds: Some(180.0),
                added_at_ms: Some(1_000),
            },
            stats: TrackStats::default(),
        }
    }

    fn feedback(track_id: u128, kind: FeedbackKind, timestamp_ms: i64) -> FeedbackEvent {
        FeedbackEvent {
            track_id,
            timestamp_ms,
            session_id: Some(1),
            kind,
        }
    }

    fn session(batch_size: usize) -> RecommendationSession {
        let tracks: Vec<Track> = (1..=20).map(|id| track(id, id, id as usize % 10)).collect();
        RecommendationSession::new(
            &tracks,
            Vec::new(),
            1_000,
            RecommendationSessionConfig {
                batch_size,
                ..RecommendationSessionConfig::default()
            },
        )
    }

    #[test]
    fn batches_grow_without_duplicates_and_stop_at_catalog_end() {
        let tracks: Vec<Track> = (1..=20).map(|id| track(id, id, id as usize % 10)).collect();
        let mut session = session(10);
        let first = session.start(&tracks, 1_000);
        assert_eq!(first.len(), 10);
        let second = session.refill(&tracks, 1_000);
        assert_eq!(second.len(), 10);
        assert!(first
            .iter()
            .all(|left| { second.iter().all(|right| left.track_id != right.track_id) }));
        assert!(session.refill(&tracks, 1_000).is_empty());
        assert_eq!(session.items().len(), 20);
        assert!(session.is_exhausted(&tracks));
    }

    #[test]
    fn skipped_item_is_removed_and_excluded() {
        let tracks: Vec<Track> = (1..=20).map(|id| track(id, id, id as usize % 10)).collect();
        let mut session = session(5);
        let first = session.start(&tracks, 1_000);
        let skipped = first[0].track_id;
        session.record_feedback(feedback(
            skipped,
            FeedbackKind::Skipped {
                listened_fraction: 0.05,
            },
            2_000,
        ));
        assert!(session.items().iter().all(|item| item.track_id != skipped));
        assert_eq!(session.events().len(), 1);
        let next = session.refill(&tracks, 2_000);
        assert!(next.iter().all(|item| item.track_id != skipped));
    }

    #[test]
    fn feedback_is_used_by_the_next_refill() {
        let tracks: Vec<Track> = (1..=20).map(|id| track(id, id, id as usize % 10)).collect();
        let mut session = session(5);
        let first = session.start(&tracks, 1_000);
        let liked = first[0].track_id;
        session.record_feedback(feedback(liked, FeedbackKind::Liked, 2_000));
        let next = session.refill(&tracks, 2_000);
        assert!(session.profile().track_score(liked).is_some());
        assert!(next.iter().all(|item| item.track_id != liked));
    }

    #[test]
    fn rerank_pending_keeps_played_items_and_returns_only_pending() {
        let tracks: Vec<Track> = (1..=20).map(|id| track(id, id, id as usize % 10)).collect();
        let mut session = session(10);
        let first = session.start(&tracks, 1_000);
        let played = first[0].track_id;
        session.mark_played(played);
        session.record_feedback(feedback(played, FeedbackKind::Liked, 2_000));

        let before: HashSet<TrackId> = session.pending_items().map(|item| item.track_id).collect();
        assert_eq!(before.len(), 9);

        let reranked = session.rerank_pending(&tracks, 3_000);
        let after: HashSet<TrackId> = reranked.iter().map(|item| item.track_id).collect();
        assert_eq!(after, before);
        assert_eq!(reranked.len(), 9);
        assert!(reranked.iter().all(|item| item.track_id != played));
        assert!(session.items().iter().any(|item| item.track_id == played));
        assert_eq!(session.items().len(), 10);
        assert_eq!(session.pending_count(), 9);

        // The next refill must not duplicate anything that survived the rerank.
        let next = session.refill(&tracks, 4_000);
        assert!(next
            .iter()
            .all(|item| !after.contains(&item.track_id) && item.track_id != played));
    }

    #[test]
    fn rerank_pending_with_nothing_pending_is_safe() {
        let tracks: Vec<Track> = (1..=20).map(|id| track(id, id, id as usize % 10)).collect();
        let mut session = session(5);
        let first = session.start(&tracks, 1_000);
        for item in first {
            session.mark_played(item.track_id);
        }
        assert!(session.rerank_pending(&tracks, 2_000).is_empty());
        assert_eq!(session.items().len(), 5);
        assert_eq!(session.pending_count(), 0);
    }

    #[test]
    fn pending_count_drops_when_tracks_are_played() {
        let tracks: Vec<Track> = (1..=20).map(|id| track(id, id, id as usize % 10)).collect();
        let mut session = session(5);
        let first = session.start(&tracks, 1_000);
        assert_eq!(session.pending_count(), 5);
        session.mark_played(first[0].track_id);
        assert_eq!(session.pending_count(), 4);
        session.record_feedback(feedback(first[1].track_id, FeedbackKind::Completed, 2_000));
        assert_eq!(session.pending_count(), 3);
    }

    #[test]
    fn empty_catalog_and_zero_batch_are_safe() {
        let mut session = session(0);
        assert!(session.start(&[], 1_000).is_empty());
        assert!(session.refill(&[], 1_000).is_empty());
        assert!(session.is_exhausted(&[]));
    }
}
