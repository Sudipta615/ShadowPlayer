//! Track ordering and the quick-filter pass over the materialised list.

use super::*;

/// Sort a materialised list by `field` and `order`.
///
/// Each track's sort key is built once into a temporary parallel array and the
/// tracks are ordered by index against it, so a sort is `O(n log n)`
/// comparisons of `&[u8]` with no allocation per comparison. The case folding is
/// the same one the repository uses when it builds rows, so `Beta` and `alpha`
/// order the same way regardless of where the list came from.
///
/// The sort keys are not stored on `Track`: they are derived index data that
/// would cost a clone of every displayed list. Building them on demand here
/// keeps `Vec<Track>::clone` cheap, which is what the 100k-track list budget
/// measures.
pub fn sort_tracks(tracks: &mut [Track], field: SortField, order: SortOrder) {
    if tracks.len() < 2 {
        return;
    }

    // Favorites first regardless of direction: "sort by favorites" with the
    // least-favorite track at the top would be a strange reading.
    if field == SortField::Favorite {
        tracks.sort_by_key(|t| std::cmp::Reverse(t.is_favorite));
        return;
    }

    let mut idx: Vec<usize> = (0..tracks.len()).collect();
    let desc = order == SortOrder::Descending;

    match field {
        SortField::Title => {
            let title = KeyTable::new(tracks, |t| &t.title);
            idx.sort_unstable_by(|&a, &b| {
                let cmp = title.get(a).cmp(title.get(b));
                if desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::Artist => {
            let artist = KeyTable::new(tracks, |t| &t.artist);
            let album = KeyTable::new(tracks, |t| &t.album);
            idx.sort_unstable_by(|&a, &b| {
                let primary = artist.get(a).cmp(artist.get(b));
                let primary = if desc { primary.reverse() } else { primary };
                primary
                    .then_with(|| album.get(a).cmp(album.get(b)))
                    .then_with(|| {
                        tracks[a]
                            .track_number
                            .unwrap_or(0)
                            .cmp(&tracks[b].track_number.unwrap_or(0))
                    })
            });
        }
        SortField::Album => {
            let album = KeyTable::new(tracks, |t| &t.album);
            idx.sort_unstable_by(|&a, &b| {
                let primary = album.get(a).cmp(album.get(b));
                let primary = if desc { primary.reverse() } else { primary };
                primary
                    .then_with(|| {
                        tracks[a]
                            .disc_number
                            .unwrap_or(1)
                            .cmp(&tracks[b].disc_number.unwrap_or(1))
                    })
                    .then_with(|| {
                        tracks[a]
                            .track_number
                            .unwrap_or(0)
                            .cmp(&tracks[b].track_number.unwrap_or(0))
                    })
            });
        }
        SortField::Duration => {
            idx.sort_unstable_by(|&a, &b| {
                let cmp = tracks[a].duration_secs.total_cmp(&tracks[b].duration_secs);
                if desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::TrackNumber => {
            let title = KeyTable::new(tracks, |t| &t.title);
            idx.sort_unstable_by(|&a, &b| {
                let primary = tracks[a]
                    .disc_number
                    .unwrap_or(1)
                    .cmp(&tracks[b].disc_number.unwrap_or(1))
                    .then_with(|| {
                        tracks[a]
                            .track_number
                            .unwrap_or(0)
                            .cmp(&tracks[b].track_number.unwrap_or(0))
                    });
                let primary = if desc { primary.reverse() } else { primary };
                primary.then_with(|| title.get(a).cmp(title.get(b)))
            });
        }
        SortField::DateAdded => {
            idx.sort_unstable_by(|&a, &b| {
                let cmp = tracks[a].added_at.cmp(&tracks[b].added_at);
                if desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::Rating => {
            idx.sort_unstable_by(|&a, &b| {
                let cmp = tracks[a].rating.cmp(&tracks[b].rating);
                if desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
        SortField::Favorite => unreachable!("handled above"),
    }

    reorder(tracks, &idx);
}

/// Contiguous flat byte buffer storing collation keys for `tracks` in 2 allocations total.
struct KeyTable {
    offsets: Vec<usize>,
    bytes: Vec<u8>,
}

impl KeyTable {
    fn new(tracks: &[Track], extract: impl Fn(&Track) -> &str) -> Self {
        let mut offsets = Vec::with_capacity(tracks.len() + 1);
        // Pre-allocate estimated byte capacity to avoid log(N) reallocations and
        // cache thrashing on dual-core CPUs during large library sorts.
        let mut bytes = Vec::with_capacity(tracks.len() * 16);
        offsets.push(0);
        for t in tracks {
            let folded = domain::text::fold_collation_key(extract(t));
            bytes.extend_from_slice(&folded);
            offsets.push(bytes.len());
        }
        Self { offsets, bytes }
    }

    #[inline]
    fn get(&self, i: usize) -> &[u8] {
        let start = self.offsets[i];
        let end = self.offsets[i + 1];
        &self.bytes[start..end]
    }
}

/// Apply the permutation `idx` to `tracks` in place.
///
/// `idx[i]` is the source position whose track belongs at destination `i`. The
/// permutation is resolved by swapping pairs until every element is home, which
/// needs neither a temporary buffer nor `Clone`.
fn reorder<T>(tracks: &mut [T], idx: &[usize]) {
    let n = tracks.len();
    // `dest[s]` is the destination of the track currently at position `s`.
    let mut dest = vec![0usize; n];
    for (destination, &source) in idx.iter().enumerate() {
        dest[source] = destination;
    }
    for i in 0..n {
        while dest[i] != i {
            let j = dest[i];
            tracks.swap(i, j);
            dest.swap(i, j);
        }
    }
}

/// Apply the active quality/format chip to a materialised track list.
pub(super) fn apply_quick_filter(
    tracks: &mut Vec<Track>,
    favorite_ids: &HashSet<TrackId>,
    recommendation_ids: &[TrackId],
    filter: QuickFilter,
) {
    match filter {
        QuickFilter::All => {}
        QuickFilter::ForYou => {
            if recommendation_ids.is_empty() {
                tracks.clear();
            } else {
                tracks.retain(|track| recommendation_ids.contains(&track.id));
            }
        }
        QuickFilter::Lossless => tracks.retain(|t| t.is_lossless),
        QuickFilter::HiRes => tracks.retain(|t| t.is_hires()),
        QuickFilter::Favorites => tracks.retain(|t| favorite_ids.contains(&t.id)),
        QuickFilter::Played => tracks.retain(|t| t.play_count > 0),
        QuickFilter::Rating => tracks.retain(|t| t.rating > 0),
    }
}
