//! Track ordering and the quick-filter pass over the materialised list.

use super::*;

/// Sort a materialised list by `field` and `order`.
///
/// Keys are case-folded once per track instead of twice per comparison, which
/// used to allocate two `String`s on every sort step. `Album` used to compare
/// case-*sensitively* while `Title` and `Artist` did not, so the same list could
/// order differently depending only on which column was chosen.
pub fn sort_tracks(tracks: &mut [Track], field: SortField, order: SortOrder) {
    // Lists built outside the repository (tests, freshly imported rows) may
    // not carry the folded keys yet. Without them the fallback compares raw
    // bytes, so `Beta` would sort before `alpha` and the column would be
    // case-sensitive depending on where the list came from.
    for track in tracks.iter_mut() {
        track.ensure_sort_keys();
    }
    match field {
        SortField::Title => {
            tracks.sort_unstable_by(|a, b| a.sort_title_bytes().cmp(b.sort_title_bytes()))
        }
        SortField::Artist => tracks.sort_unstable_by(|a, b| {
            a.sort_artist_bytes()
                .cmp(b.sort_artist_bytes())
                .then_with(|| a.sort_album_bytes().cmp(b.sort_album_bytes()))
                .then_with(|| {
                    a.track_number
                        .unwrap_or(0)
                        .cmp(&b.track_number.unwrap_or(0))
                })
        }),
        SortField::Album => tracks.sort_unstable_by(|a, b| {
            a.sort_album_bytes()
                .cmp(b.sort_album_bytes())
                .then_with(|| a.disc_number.unwrap_or(1).cmp(&b.disc_number.unwrap_or(1)))
                .then_with(|| {
                    a.track_number
                        .unwrap_or(0)
                        .cmp(&b.track_number.unwrap_or(0))
                })
        }),
        SortField::Duration => {
            tracks.sort_unstable_by(|a, b| a.duration_secs.total_cmp(&b.duration_secs))
        }
        SortField::TrackNumber => tracks.sort_unstable_by(|a, b| {
            a.disc_number
                .unwrap_or(1)
                .cmp(&b.disc_number.unwrap_or(1))
                .then_with(|| {
                    a.track_number
                        .unwrap_or(0)
                        .cmp(&b.track_number.unwrap_or(0))
                })
                .then_with(|| a.sort_title_bytes().cmp(b.sort_title_bytes()))
        }),
        SortField::DateAdded => tracks.sort_unstable_by_key(|t| t.added_at),
        // Favorites first regardless of direction: "sort by favorites" with
        // the least-favorite track at the top would be a strange reading.
        SortField::Favorite => {
            tracks.sort_by_key(|t| std::cmp::Reverse(t.is_favorite));
            return;
        }
        SortField::Rating => tracks.sort_unstable_by_key(|t| t.rating),
    }
    if order == SortOrder::Descending {
        tracks.reverse();
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
