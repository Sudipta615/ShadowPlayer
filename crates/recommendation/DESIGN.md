# Design notes and evaluation plan

## Design decisions

### Three-region sampling

Use 6 seconds at the beginning, centered middle, and end by default (18 seconds
total). Three windows give better coverage than only sampling the beginning
while avoiding the cost of full-track analysis. When the file is shorter than
three nominal windows, shorten each window to one third of duration. On formats
with poor seeking, the adapter can cache a decoder index or fall back to one
sequentially decoded short sample; it must not quietly decode the full track
three times.

A failed window is counted in `AnalysisResult.windows_failed` but does not
discard the remaining windows. If every window fails, the last decoder error is
returned. This keeps one damaged region from making an otherwise analyzable
track disappear from the library.

### One pass, shared transforms

Each window is downmixed and low-pass-resampled to the profile's target rate
using a 128-phase Blackman-windowed-sinc coefficient table with rate-dependent
support. The resampler normalizes each phase over only the taps that exist at
the signal edges, preserving DC without amplifying a zero-padded tail. Plans
are cached by `(input_rate, output_rate)` and reused across tracks.

Stereo downmix computes mid and side energy. It normally uses mid, but switches
to side when mid is almost fully cancelled. This avoids turning an anti-phase
stereo master into near-silence without changing the result for ordinary
correlated stereo. Multichannel input falls back to an average; a
channel-layout-aware decoder-side downmix is preferred when available.

Analysis uses a periodic Hann window and a precomputed real radix-2 FFT with a
512-sample hop. The Hann window, FFT twiddles, frame buffer, power spectrum, and
chroma filterbank are allocated once per track analysis, not once per frame.
This avoids forcing in a DSP dependency, but it is an implementation trade-off:
validate numerical behavior against `rustfft` or Bliss on a music corpus before
promising feature parity.

Frame gating uses an absolute floor (`-72 dBFS`) and a relative floor (`35 dB`
below the loudest frame). Overall loudness and dynamic range use all frames;
timbre descriptors use only gated frames, so a quiet intro does not flatten the
spectral statistics of a loud track. Onset strength uses positive log-magnitude
change and is retained across gated frames for tempo estimation.

### 20 features, not a giant learned embedding

The representation prioritizes inexpensive, explainable descriptors:

- overall level and dynamics;
- spectral shape/timbral descriptors;
- a low-frequency proportion;
- rough tempo with a confidence gate;
- 12 chroma bins for pitch-class/harmonic content.

Use track-wise values, not raw per-frame features, for the first ranker. A
20-dimensional `f32` vector occupies 80 bytes; version/mask overhead makes its
persistent representation 88 bytes. The host can store vectors as a SQLite BLOB
and query neighboring tracks without an embedding runtime. ShadowPlayer's UUID
newtypes map losslessly to `u128`; string genre labels use a deterministic
64-bit key for in-memory affinity statistics while the source labels remain in
the main database.

### Similarity and missing data

Features are normalized to comparable ranges. Weighted RMS distance is averaged
only over dimensions valid in both rows. Unreliable tempo is marked absent;
silence cannot turn missing chroma into a false match. Scalar dimensions and
chroma are both treated as normalized dimensions, with chroma receiving a
combined share of the weight budget.

### Tuning-aware chroma

A recording can be consistently sharp or flat. The analyzer estimates a tuning
offset in cents by finding interpolated spectral peaks, mapping them to
fractional MIDI values, and averaging their deviation on a one-semitone circle.
The chroma filterbank then shifts by that offset. Each linear FFT bin
contributes to nearby pitch classes through a Gaussian kernel, and a `1/f` gain
compensates for the larger number of high-frequency bins per semitone. This is
an inexpensive approximation of constant-Q chroma, not a full log-frequency
transform.

### Tempo confidence

Tempo is estimated from a log-magnitude onset envelope using normalized
autocorrelation. Octave ambiguity is handled by folding candidate periods
toward the most confident window estimate and averaging only candidates that
agree after folding. The tempo feature is persisted only when confidence
reaches `min_tempo_confidence` (default `0.18`); otherwise the validity bit is
cleared and similarity ignores tempo. This is deliberately conservative: a
wrong tempo is more harmful than a missing one.

### Learning without a neural network

The ranker uses weighted signals and two online-friendly profiles:

- positive feature centroid;
- negative feature centroid.

Interactions are time-decayed exponentially so current listening preferences
can evolve. Likes and dislikes have stronger effect than play starts; an early
skip is more negative than a late skip. Ingest progress at milestones or
aggregate it rather than counting every UI timer tick as a fresh signal.

Before contributing to the profile, each track's decayed evidence is saturated
at `max_track_weight` (default `3.0`). This prevents a single heavily repeated
track from dominating feature centroids or affinity tables. Direct track,
genre, artist, and album affinities are learned from the same signed events.
Affinity ratios use additive smoothing and a saturating `tanh` transform, so a
single like does not produce a perfect score and strong dislikes can suppress a
track or its relatives.

The session transition table is a directed co-occurrence count from one
`PlayStarted` event to the next in the same session, limited by
`transition_gap_ms` (default three hours). It is cheap and interpretable, but
should only influence sequencing after it has observations. It should not be
described as collaborative filtering.

### Candidate ranking and diversity

First score candidates with a weighted average of available evidence. The
weighted average is renormalized per candidate, preventing a track with missing
features from receiving fake zeroes. Novelty and freshness are always
available, so they are excluded from the evidence-coverage denominator. The
renormalized relevance is then shrunk toward `prior_score` in proportion to
missing evidence:

```text
coverage = available_evidence_weight / configured_evidence_weight
base_score = prior_score + (renormalized_relevance - prior_score) * coverage
```

This prevents a sparse candidate from winning merely because novelty was its
only active signal. `Recommendation.confidence` and
`ScoreBreakdown.coverage` expose the coverage value.

After that, apply recency and diversity adjustment using a greedy
maximal-marginal-relevance style step:

```text
adjusted = base_score - recent_penalty - λ × max_similarity_to_selected - artist_repeat_penalty
```

This reduces repeated artists and near-duplicates without needing an expensive
optimizer. The candidate pool is capped to keep sequence selection predictable.
Instead of recomputing similarity against every selected track on every MMR
step, the implementation keeps a running maximum similarity per candidate.
Complexity is `O(N × 20 + N log P + P log P + K × P × 20)` for `N` tracks, a
pool of `P` candidates, and `K` results.

### Stateful session layer

`Recommender` stays stateless: one call ranks one catalog for one request and
returns one batch. That makes it easy to test, cache, and call from different
surfaces, but it does not by itself provide the “starts with 10–15 tracks, then
grows while I listen” product behavior.

`RecommendationSession` owns the small amount of state that behavior needs:

- the rolling list shown in the UI;
- the set of tracks already recommended, played, or explicitly excluded;
- accumulated `FeedbackEvent` history;
- recent playback and artist context for recency/diversity.

The lifecycle is host-driven:

```text
new() -> start() -> mark_played()/record_feedback() -> refill() -> ...
```

`start()` is the first `refill()`. `refill()` rebuilds `TasteProfile` from all
accumulated events, ranks the catalog with the current seed and context, and
appends only tracks that were never recommended or played. This is why feedback
changes the next batch without resetting or duplicating the visible list.

`record_feedback()` applies immediate list hygiene: skipped and disliked tracks
are removed from the displayed list and excluded from future batches. Other
events update history and playback context. The profile is rebuilt lazily on
the next ranking call, not on every feedback event, so a burst of playback
events does not trigger repeated full-catalog scans.

`rerank_pending()` is the optional middle ground between “only append” and
“reset the list.” It rebuilds the profile and re-ranks only the unplayed part of
the current list, preserving played items for history. Use it when feedback
should visibly reorder upcoming tracks; use `refill()` when the pending queue is
short and new tracks should be appended.

Sessions are intended for one listening session or one UI view, not as a
long-lived mutable catalog. Recreate the session when the library changes
substantially, the user changes the seed, clears feedback, or toggles the
feature off and on. The session starts no threads and performs no I/O; the host
still owns persistence, the optional on/off setting, analysis scheduling, and
rendering.

### Feature schema migration

The feature semantics changed with tuning-aware chroma, gated frame statistics,
and harmonic tempo estimation, so the schema was bumped from `SRF1` to `SRF2`.
The blob remains 88 bytes, but the magic changes. Hosts must invalidate cached
`SRF1` rows and recompute them; silently decoding `SRF1` values as `SRF2` would
mix incompatible chroma and tempo distributions.

## Validation in this workspace

Release-mode unit tests cover resampling, anti-phase downmix, FFT correctness,
frame gating, tuning, tempo, feature codec, profile saturation, transitions,
affinity learning, coverage shrinkage, determinism, ranking safety, and rolling
session behavior (batch growth, duplicate prevention, feedback removal,
pending counts, and reranking). Clippy passes with `-D warnings`.

Synthetic benchmarks provide regression baselines, not universal latency
claims:

- ranking 100,000 tracks at `limit = 30`: p50 about 40 ms, p95 about 42 ms;
- profile build for 100,000 tracks: about 14 ms;
- three-window analysis of a 24-second 44.1 kHz stereo signal: about 64 ms p50
  on `Balanced`, excluding decoder seek/IO.

The numbers depend on CPU, compiler, allocator, and the synthetic distribution.
Run the examples on target hardware and with the real decoder before making
product claims.

## Accuracy improvement roadmap

1. **Baseline corpus:** use a consented local collection with user-assigned
   “similar/not similar” pairs and at least 100 track-radio sessions. Don't use
   one hand-selected music genre as the only test.
2. **Compare against Bliss:** analyze the same tracks with the current Bliss
   release and this 20-feature engine. Evaluate nearest-neighbor overlap, human
   relevance judgments, and measured analysis costs. The representations are
   different, so similarity scores themselves are not directly comparable.
3. **Sampling disagreement:** extract each of the three region vectors
   separately in an experimental build. If their pairwise disagreement is high,
   schedule one additional sample or a full analysis during idle time. Keep the
   initial production path fixed at three windows until validated.
4. **Weights:** tune feature weights and ranking weights on a held-out set.
   Avoid using user events from the test window to construct the profile; that
   would leak the answer.
5. **Metrics:** measure Precision@K / NDCG@K on judgments, skip rate
   conditional on exposure, completion rate, save/like rate, artist entropy,
   catalog coverage, novelty, p50/p95 latency, CPU time, peak RSS, and playback
   underruns.
6. **Online adaptation:** only tune from feedback after a sufficient sample
   count. A small pairwise linear ranker can be introduced later; serialize
   learned coefficients and run it on candidate features. Do not add LSTM/ONNX
   by default.
7. **Hardware tiers:** choose sampling profiles by user setting or startup
   benchmark, not CPU model name alone. SSD/HDD effects are primarily about
   seek/index I/O; do not run library-wide analysis during playback or while
   database writes are congested.

## External inspiration

- [Bliss-rs](https://github.com/Polochon-street/bliss-rs): analyze songs into
  feature vectors, compare them using distances, persist analysis, and
  optionally learn a distance metric. Its README documents 22,050 Hz mono float
  PCM and analysis caching.
- [VecStore recommender API](https://docs.rs/vecstore/latest/vecstore/recommender/):
  content-based, collaborative and hybrid recommendation strategies over
  vectors. A vector store is not required for a 20-D feature vector and a local
  100k-track scan; benchmark before adding HNSW/quantization complexity.
- [OxiMedia Recommend](https://docs.rs/oximedia-recommend/latest/oximedia_recommend/):
  useful inspiration for modular ranking, user profiles, feedback, diversity,
  novelty and hybrid scores. This implementation deliberately avoids importing
  its broad dependency surface.
- [sbr](https://docs.rs/sbr/latest/sbr/): sequential recommendations, including
  a faster EWMA alternative to LSTM. This implementation uses a smaller
  session-transition table as the first sequential signal.
- [Spotify Engineering: candidate generation and ranking](https://engineering.atspotify.com/2021/11/the-rise-and-lessons-learned-of-ml-models-to-personalize-content-on-home-part-i):
  reinforces the separation between finding candidates and ordering them; this
  project adapts that architecture for a single-user local library.
