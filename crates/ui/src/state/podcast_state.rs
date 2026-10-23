use database::PodcastRepo;
use domain::ids::{PodcastEpisodeId, PodcastFeedId};
use domain::models::{PodcastEpisode, PodcastFeed};

/// Everything the Podcasts view needs between frames.
///
/// Feeds and episodes are cached here because the view is rebuilt on every
/// frame and the database is not. The state owns no worker; downloads and
/// refreshes live in [`crate::services::PodcastService`].
#[derive(Default)]
pub struct PodcastState {
    pub feeds: Vec<PodcastFeed>,
    pub selected_feed: Option<PodcastFeedId>,
    pub episodes: Vec<PodcastEpisode>,
    /// Subscription URL typed into the add-feed field.
    pub new_feed_url: String,
    /// The episode the transport is currently playing, if any.
    pub playing_episode: Option<PodcastEpisodeId>,
    /// Position of the podcast transport, in seconds.
    pub position_secs: f32,
    /// Duration of the playing episode, in seconds.
    pub duration_secs: f32,
    /// Whether the podcast transport is playing.
    pub is_playing: bool,
    /// Last position written to the database, so progress is saved every few
    /// seconds rather than on every frame.
    pub last_saved_secs: f32,
    /// A failed operation, shown under the header until the next one.
    pub error: Option<String>,
}

impl PodcastState {
    /// Reload the feed list, keeping the selection when it still exists.
    pub fn reload_feeds(&mut self, repo: &PodcastRepo) {
        match repo.list_feeds() {
            Ok(feeds) => {
                if self
                    .selected_feed
                    .is_none_or(|id| !feeds.iter().any(|f| f.id == id))
                {
                    self.selected_feed = feeds.first().map(|f| f.id);
                }
                self.feeds = feeds;
            }
            Err(e) => {
                self.error = Some(tr_args!(
                    "Could not load podcast feeds: {error}",
                    "error" => e
                ))
            }
        }
        self.reload_episodes(repo);
    }

    /// Reload the episodes of the selected feed.
    pub fn reload_episodes(&mut self, repo: &PodcastRepo) {
        let Some(id) = self.selected_feed else {
            self.episodes.clear();
            return;
        };
        match repo.list_episodes(&id) {
            Ok(episodes) => self.episodes = episodes,
            Err(e) => {
                self.error = Some(tr_args!(
                    "Could not load podcast episodes: {error}",
                    "error" => e
                ))
            }
        }
    }

    pub fn select_feed(&mut self, repo: &PodcastRepo, id: PodcastFeedId) {
        self.selected_feed = Some(id);
        self.reload_episodes(repo);
    }

    pub fn selected_feed(&self) -> Option<&PodcastFeed> {
        self.feeds
            .iter()
            .find(|feed| Some(feed.id) == self.selected_feed)
    }

    pub fn episode(&self, id: PodcastEpisodeId) -> Option<&PodcastEpisode> {
        self.episodes.iter().find(|episode| episode.id == id)
    }
}
