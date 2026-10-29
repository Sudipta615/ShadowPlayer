# Shadow Recommender

A lightweight, native Rust music-analysis and recommendation core designed for
ShadowPlayer. It is wired into ShadowPlayer's existing library, queue, and
decoder APIs behind the `For You` feed and the Recommendation Engine settings
card; it is not a Python service, a model download, or a dependency on a hosted
recommender.

## Goals

- No Python runtime, model download, external process, or mandatory third-party crate.
- Analyze three short regions instead of decoding every second of a long track.
- Use an anti-phase-safe downmix and anti-alias-filtered resampling before spectral analysis.
- Produce 20 compact features: seven acoustic descriptors, one tempo descriptor, and 12 pitch-class/chroma values.
- Persist a feature vector in 88 bytes and reuse it until the track changes or the feature schema version changes.
- Blend seed-track similarity, learned taste, direct track/album affinity, engagement, genre/artist affinity, session transitions, novelty, freshness, recency, and diversity.
- Leave decoding, database operations, concurrency, thread priorities, cancellation, and UI work to ShadowPlayer. The crate starts no threads and never touches the playback callback.

## Current status and verification

The crate is dependency-free, synchronous, and currently validated in this
workspace with:

```text
cargo test --release                     47 passed
cargo clippy --release --all-targets -- -D warnings   clean
```

The recommendation and analysis benchmarks below are synthetic and
machine-specific. They are useful for regression checks, not as universal
latency claims. There is not yet a consented real-music relevance corpus, so
the default weights remain tunable priors.

## How track analysis works

Default `Balanced` settings:

| Setting | Value |
|---|---:|
| Target sample rate | 11,025 Hz, mono |
| Regions | Start, middle, end |
| Duration per region | 6 seconds |
| Total audio analyzed | Up to 18 seconds per track |
| FFT size | 2,048 samples |
| Hop size | 512 samples |
| Feature dimensions | 20 |
| Model/runtime | None |

For tracks shorter than 18 seconds, each region shrinks to one third of the
track, so the three regions cover the available audio. For longer tracks, the
analyzer seeks to the beginning, center, and end. The decoder adapter is
expected to decode only the requested interval.

Each region is downmixed and resampled with a rate-adaptive
Blackman-windowed-sinc filter using a 128-phase coefficient table. The filter
support expands for aggressive downsampling, and plans are cached by
`(input_rate, output_rate)`. Stereo downmix evaluates both mid and side energy
and uses the side signal only when the mid signal is almost completely
cancelled, so an anti-phase master does not disappear from the analyzer.

Analysis then uses a periodic Hann window and a real radix-2 FFT. The analyzer
computes frame statistics in one pass, reusing buffers across frames and
regions. Frames below `-72 dBFS` or more than `35 dB` below the loudest frame
are excluded from spectral timbre statistics, while overall loudness and
dynamic range still describe the whole sampled signal. A failed region does not
discard the other two; `AnalysisResult` reports `windows_analyzed` and
`windows_failed`.

### Tuning, chroma, and tempo

- Tuning is estimated from interpolated spectral peaks over a one-semitone
  circle. The resulting `tuning_cents` offset shifts a Gaussian chroma
  filterbank covering 55–2,000 Hz, with `1/f` gain to approximate constant-Q
  resolution. This keeps detuned recordings from smearing across pitch classes.
- Tempo uses a log-magnitude onset envelope and normalized autocorrelation.
  Candidate periods are folded across octave ambiguity, and tempo is persisted
  only when `tempo_confidence >= min_tempo_confidence` (default `0.18`).
- `AnalysisResult` exposes `tempo_confidence`, `tuning_cents`,
  `target_sample_rate`, and `sampled_seconds` for diagnostics and UI/debug
  views. Only the 88-byte feature vector is required for ranking.

### Hardware profiles

| Profile | Sample rate | Region length | FFT / hop | Total sampled audio |
|---|---:|---:|---:|---:|
| `LowResource` | 8,000 Hz | 4 s | 1,024 / 512 | up to 12 s |
| `Balanced` | 11,025 Hz | 6 s | 2,048 / 512 | up to 18 s |
| `HighQuality` | 16,000 Hz | 6.5 s | 2,048 / 512 | up to 19.5 s |

These are quality/cost presets, not benchmark claims. The host should measure
analysis latency and CPU use on representative hardware and make indexing
idle-priority, cancellable, and incremental.

### Feature vector (schema `SRF2`)

`FEATURE_SCHEMA_VERSION` is `2`; the serialized magic is `SRF2`. Cached `SRF1`
vectors have different chroma/tempo semantics and must be discarded and
recomputed.

The 20 values are normalized to `[0, 1]`:

1. Mean loudness (normalized dBFS)
2. Dynamic range (10th-to-90th percentile RMS range)
3. Spectral centroid
4. 85% spectral rolloff
5. Spectral flatness
6. Positive spectral flux
7. Low-frequency energy ratio (30–250 Hz / energy above 30 Hz)
8. Tempo estimate (normalized 50–200 BPM; omitted from similarity when unreliable)
9–20. Twelve pitch-class chroma bins (C through B)

Chroma values are normalized across the 12 bins. `valid_mask` marks
missing/unstable dimensions, so uncertain tempo or silent chroma is ignored
rather than being treated as a real zero. Similarity is
`1 - weighted_RMS_distance`, renormalized over dimensions present in both
tracks. The feature blob is 88 bytes: four-byte `SRF2` magic, four-byte
validity mask, and 20 little-endian `f32` values.

The host should key cached rows by track ID plus file fingerprint and schema
version. Track/artist IDs are represented as `u128` internally so ShadowPlayer
UUIDs can round-trip without hashing; genre labels use a normalized 64-bit
FNV-1a key for compact in-memory affinity lookups. All recommender timestamps
are Unix milliseconds; ShadowPlayer's metadata extractor stores `added_at` in
Unix seconds, so multiply by 1,000 before setting `added_at_ms`. Convert
`last_played_at` and event/scrobble timestamps similarly when the originating
database value is in seconds.

## Recommendation and personalization

The engine is a hybrid ranker, not one monolithic model. The default weights
sum to `1.0`:

| Signal | Weight |
|---|---:|
| Seed-track audio similarity | 0.24 |
| Personal feature profile | 0.18 |
| Direct track affinity | 0.10 |
| Explicit/implicit engagement | 0.14 |
| Genre affinity | 0.06 |
| Artist affinity | 0.04 |
| Album affinity | 0.02 |
| Same-session next-track transition | 0.10 |
| Historical play affinity | 0.03 |
| Novelty | 0.05 |
| Recently added to library | 0.04 |
| **Total** | **1.00** |

Weights are renormalized across signals available for a particular track. For
example, a track without audio analysis does not receive a fake zero
similarity; its content weight is omitted for that candidate. After
renormalization, the score is shrunk toward `prior_score` (default `0.5`) in
proportion to missing evidence:

```text
coverage = available_evidence_weight / configured_evidence_weight
base_score = prior_score + (renormalized_relevance - prior_score) * coverage
```

Novelty and freshness are always available and are deliberately excluded from
the coverage denominator. Without this shrinkage, a sparse candidate could win
merely because novelty was its only active signal. `Recommendation.confidence`
exposes the same coverage value, and `ScoreBreakdown.coverage` makes it
inspectable.

The taste profile uses signed signals and exponential time decay (45-day
default half-life):

- Like: `+2.0`
- Dislike: `-3.0`
- Completed: `+0.70`
- Replay: `+0.90`
- Added to playlist: `+0.80`
- Skip before 10%: `-1.20`
- Skip before 35%: `-0.80`
- Skip before 70%: `-0.35`
- Weak play-start signal: `+0.12`
- Rating: `(rating - 3) × 0.70` on a 0–5 scale

These are tunable starting priors, not universal truth. Don't treat every skip
as a dislike: its signal depends on how much of the track was heard. When
recording progress, emit only meaningful milestones (for example 10%, 50%,
80%) or aggregate updates per track/session; logging every playback timer tick
would overcount the same listening decision.

Each track's decayed evidence is saturated at `max_track_weight` (default
`3.0`) before it contributes to feature centroids or affinity tables. One
heavily repeated track therefore cannot dominate the whole profile. The
profile learns positive and negative feature centroids plus genre, artist,
album, and direct track affinities. Affinity ratios use additive smoothing and
a saturating `tanh` transform, so one like does not produce a perfect score and
strong dislikes can suppress a track or its relatives.

For tracks with session IDs, `PlayStarted` events also learn directed
transitions (A → B). Transitions are counted only within a session and with
gaps no longer than `transition_gap_ms` (default three hours). That signal is
useful for sequencing a radio queue; it is intentionally much simpler than an
LSTM. Rebuild/cache the profile when new events arrive instead of rebuilding it
on each UI repaint.

The freshness signal uses the library-added timestamp, not a claimed release
date. The top-level score is a weighted average over available signals. Then
recency and diversity penalties are applied:

```text
final_score = clamp(
    base_score
    - recent_play_penalty
    - diversity_lambda * max_similarity_to_selected
    - artist_repeat_penalty,
    0, 1
)
```

The candidate shortlist is capped at `candidate_pool_max` (default 512) and
`limit × candidate_pool_factor` (default factor 8), so diversity-aware
selection runs on a small pool rather than repeatedly comparing against the
entire library. The default `diversity_lambda` is `0.15`; the recent-play
penalty is `0.22` with a 3-day half-life, and a recent skip receives a stronger
short-term cooldown. MMR selection maintains a running maximum similarity per
candidate, giving `O(N × 20 + N log P + P log P + K × P × 20)` work instead
of recomputing every selected-pair similarity on every step.

## Stateful “For You” sessions

`Recommender::recommend()` is stateless: it ranks a catalog for one request.
That is the right primitive for a queue refresh, but it is not enough for a
Spotify/YouTube-style list that starts small, learns during playback, and grows
as the listener interacts.

Use `RecommendationSession` for that product behavior. It owns:

- the rolling recommendation list;
- already recommended, played, and excluded track IDs;
- the feedback history used to rebuild the taste profile;
- recent playback and artist context.

A typical host flow:

```rust
use shadow_recommender::{
    FeedbackEvent, FeedbackKind, RecommendationSession, RecommendationSessionConfig,
};

let mut session = RecommendationSession::new(
    &tracks,
    existing_feedback,
    now_ms,
    RecommendationSessionConfig {
        batch_size: 12,
        seed_track_id: Some(current_track_id),
        ..RecommendationSessionConfig::default()
    },
);

// First click on the Songs-table recommendation button.
let first_batch = session.start(&tracks, now_ms);

// As playback happens:
session.mark_played(first_batch[0].track_id);
session.record_feedback(FeedbackEvent {
    track_id: first_batch[0].track_id,
    timestamp_ms: now_ms + 30_000,
    session_id: Some(session_id),
    kind: FeedbackKind::Liked,
});

// When the pending list gets short, request the next batch.
let next_batch = session.refill(&tracks, now_ms + 60_000);
```

Behavior:

- `start()` returns the first 10–15 tracks (configurable `batch_size`).
- `record_feedback()` stores likes, skips, ratings, completions, replays, and
  play starts. Skipped/disliked tracks disappear from the displayed list
  immediately.
- `refill()` rebuilds the taste profile from accumulated feedback, then appends
  new tracks using the updated profile. Previously recommended or played tracks
  are never returned again.
- `rerank_pending()` optionally reorders only the unplayed part of the list
  after new feedback, while preserving played tracks for history.
- `pending_count()` lets the host auto-refill when the remaining queue is low.
- `items()` contains the full rolling list, including played tracks;
  `pending_items()` returns only unplayed recommendations.

The session still performs no I/O and starts no threads. The host remains
responsible for persistence, the optional on/off setting, analysis scheduling,
and UI rendering.

### Product integration outline

To match the intended ShadowPlayer behavior:

1. Add a `recommendation.enabled` setting and a dedicated settings section for
   analysis profile, batch size, auto-refill threshold, and feedback options.
2. Add an “Analyze library” action that runs `analyze_track` on a low-priority
   worker and stores the 88-byte feature vectors.
3. Add a “For You”/“Recommended” button next to Lossless/Hi-Res in the Songs
   table. On first click, create a session and show its first batch.
4. Hook the player’s play-start, skip, completion, like, dislike, and rating
   actions into `mark_played()`/`record_feedback()`.
5. When `pending_count()` falls below the threshold, call `refill()` and append
   the new batch. This gives the “keeps adding songs as I listen” behavior.
6. Rebuild/reopen the session when the user changes the seed, clears feedback,
   or disables the feature.

See `examples/session_demo.rs` for a runnable demonstration.

## Minimal integration flow

1. Implement `AudioWindowSource` using ShadowPlayer's existing decoder. Its
   `read_window(start, duration)` must seek and return interleaved PCM only for
   that interval. It should not decode the whole file on each call.
2. Run `analyze_track()` on one low-priority background worker during
   import/idle time. The analyzer requests target-rate output indirectly by
   calling the crate's downmix/resampler on each decoded window.
3. Save `AnalysisResult.features.to_bytes()` in the existing SQLite database.
   The blob is exactly 88 bytes and includes its format magic/version and valid
   mask. Keep a file fingerprint (for example size + mtime, or a stronger
   fingerprint) to invalidate stale features. Discard `SRF1` rows after this
   upgrade.
4. Construct/reuse `Track` rows from your existing database, passing cached
   features and play stats. ShadowPlayer's `TrackId` and `ArtistId` wrap UUIDs;
   map them losslessly with `.0.as_u128()` when entering this crate, and restore
   them with `Uuid::from_u128(id)` when consuming recommendations. The current
   domain `Track` stores artist as a string rather than an `ArtistId`; use the
   resolved domain artist ID from the database if available, or
   `domain::ArtistId::from_name(&track.artist)` for the model's canonical
   name-based ID. For the current `Option<String>` genre, call `genre_key(label)`
   and collect the optional result into `genre_ids`.
5. For a one-shot refresh, rebuild `TasteProfile::build(...)` and call
   `Recommender::recommend(...)`. For a rolling “For You” list, create a
   `RecommendationSession`, call `start()`, then feed playback events through
   `mark_played()` and `record_feedback()` and call `refill()` when the pending
   list gets short.
6. Consume `RecommendationReason` flags for explanation text and
   `Recommendation.confidence` for diagnostics in the UI.
7. Never call analysis, database queries, or recommendation ranking from the
   real-time audio callback.

## API example

```rust
use shadow_recommender::{
    analyze_track, AnalysisConfig, AnalysisProfile, RecommendationRequest, Recommender,
    TasteProfile, TasteProfileConfig,
};

// `source` implements AudioWindowSource around ShadowPlayer's decoder.
let analysis = analyze_track(
    &mut source,
    &AnalysisConfig::for_profile(AnalysisProfile::Balanced),
)?;
let blob = analysis.features.to_bytes();

// `tracks` are populated from ShadowPlayer's existing library/cache.
// `events` are local likes, skips, ratings, play starts, completions, etc.
let now_ms = 1_800_000_000_000_i64;
let profile = TasteProfile::build(
    &tracks,
    &events,
    now_ms,
    &TasteProfileConfig::default(),
);
let request = RecommendationRequest {
    seed_track_id: Some(current_track_id),
    limit: 30,
    now_ms,
    excluded_track_ids: &queued_track_ids,
    recently_played_ids: &recent_track_ids,
    recent_artist_ids: &recent_artist_ids,
};
let recommendations = Recommender::default().recommend(&tracks, &profile, &request);
for recommendation in recommendations {
    // recommendation.track_id, .score, .base_score, .confidence, .reasons, .breakdown
}
```

See `examples/recommendation_demo.rs` for a complete ranking example without an
audio decoder, `examples/session_demo.rs` for a rolling recommendation session,
`examples/benchmark_recommendation.rs` for ranking throughput, and
`examples/benchmark_analysis.rs` for a synthetic analysis benchmark.

## Integration caveats

- `AudioWindowSource` is an adapter trait; this crate does not implement
  ShadowPlayer's Symphonia seeking/decoding adapter because that adapter's exact
  workspace API must be wired in within ShadowPlayer.
- Some container/codec combinations seek to an earlier packet and decode
  forward. The adapter must trim decoded samples to the requested region.
- The three sampled regions make analysis substantially cheaper than full-track
  analysis, but may miss a bridge, solo, late drop, or abrupt genre change. A
  future adaptive mode can analyze one extra region when the three segments
  disagree strongly.
- Tempo estimation from a six-second segment is inherently uncertain. The
  validity mask suppresses it when periodicity is weak; don't use tempo as a
  hard filter.
- Chroma helps with tonal/harmonic resemblance but isn't a learned semantic
  embedding and doesn't understand lyrics or artist identity.
- The current transitions are local, single-listener transitions. Collaborative
  filtering across users requires a separate permissioned dataset and should
  remain optional.
- `Recommender::recommend()` is synchronous. Use the host worker pool and
  coalesce/cancel stale requests when users switch tracks quickly.

## Validation and microbenchmarks

```bash
cargo fmt --all -- --check
cargo test --release
cargo clippy --release --all-targets -- -D warnings
cargo run --release --example benchmark_recommendation -- 100000 30 7
cargo run --release --example benchmark_analysis
```

Measured in this workspace with the release profile (machine-specific, seven
ranking iterations, `limit = 30`):

| Library size | Profile build | Ranking p50 | Ranking p95 |
|---:|---:|---:|---:|
| 10,000 tracks | 1.1 ms | 4.5 ms | 5.2 ms |
| 50,000 tracks | 4.6 ms | 20.7 ms | 24.4 ms |
| 100,000 tracks | 14.0 ms | 40.5 ms | 42.2 ms |

Synthetic analysis of a 24-second, 44.1 kHz stereo signal (three windows,
release build, excludes decoder seek/IO):

| Profile | p50 | p95 | Sampled audio |
|---|---:|---:|---:|
| `LowResource` | 34.1 ms | 35.9 ms | 12.0 s |
| `Balanced` | 63.8 ms | 64.7 ms | 18.0 s |
| `HighQuality` | 79.8 ms | 87.2 ms | 19.5 s |

Run the benchmarks with `10000`, `50000`, and `100000` on each target hardware
tier and record the CPU model, OS, compiler version, elapsed time, and peak
RSS. The host should separately benchmark:

1. decode/seek and resample one 6-second region;
2. three-region feature analysis;
3. ranking a 10k, 50k, and 100k track library;
4. ranking with a full 512-candidate MMR pool;
5. playback/DSP running concurrently with one low-priority analysis job.

Record CPU time, wall time, peak RSS, UI frame-time impact, decoder seek
behavior, and the quality metrics described in `DESIGN.md`. Do not publish
latency numbers as universal claims; measure on the target device and with the
real decoder.
