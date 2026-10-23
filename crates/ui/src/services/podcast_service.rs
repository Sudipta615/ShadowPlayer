//! Podcast subscriptions: RSS/Atom parsing, feed refresh and episode download.
//!
//! Podcasts are deliberately separate from the music library. An episode has
//! its own download and resume state, and a removed feed must not disturb a
//! single track, playlist or play count.
//!
//! Network work happens on one worker thread at a time. The UI never blocks on
//! a feed, and an offline machine simply reports an error it can retry.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use database::PodcastRepo;
use domain::ids::{PodcastEpisodeId, PodcastFeedId};
use domain::models::{PodcastEpisode, PodcastFeed};

const MAX_FEED_BYTES: u64 = 8 * 1024 * 1024;
const MAX_EPISODE_BYTES: u64 = 512 * 1024 * 1024;
const USER_AGENT: &str = concat!("ShadowPlayer/", env!("CARGO_PKG_VERSION"));

#[derive(Debug)]
pub enum PodcastEvent {
    /// A feed and its episodes were written; the UI should reload.
    FeedUpdated { feed_id: PodcastFeedId },
    /// An episode finished downloading.
    Downloaded { episode_id: PodcastEpisodeId },
    /// A task failed. The message is ready for a toast.
    Error(String),
}

/// A parsed feed, before it is matched against what is already stored.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFeed {
    pub title: String,
    pub description: Option<String>,
    pub artwork_url: Option<String>,
    pub episodes: Vec<ParsedEpisode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEpisode {
    pub guid: String,
    pub title: String,
    pub description: Option<String>,
    pub enclosure_url: String,
    pub published_at: Option<i64>,
    pub duration_secs: Option<f32>,
}

enum Job {
    Refresh {
        feed_id: PodcastFeedId,
        url: String,
        result: Result<ParsedFeed, String>,
    },
    Download {
        episode: PodcastEpisode,
        result: Result<PathBuf, String>,
    },
}

pub struct PodcastService {
    repo: PodcastRepo,
    download_dir: PathBuf,
    inflight: Option<(String, Receiver<Job>)>,
    status: String,
}

impl PodcastService {
    pub fn new(repo: PodcastRepo, data_dir: &Path) -> Self {
        let download_dir = data_dir.join("podcasts");
        let _ = std::fs::create_dir_all(&download_dir);
        Self {
            repo,
            download_dir,
            inflight: None,
            status: String::new(),
        }
    }

    pub fn is_busy(&self) -> bool {
        self.inflight.is_some()
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }

    /// Subscribe to a URL. Returns the feed once it has been fetched and
    /// stored; the UI picks up the result through [`Self::poll`].
    pub fn add_feed(&mut self, url: &str) -> Result<(), String> {
        let url = url.trim();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(tr!("Enter an http:// or https:// podcast feed URL."));
        }
        if self.is_busy() {
            return Err(tr!("Another podcast task is still running."));
        }
        if self
            .repo
            .get_feed_by_url(url)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Err(tr!("That feed is already subscribed."));
        }

        let feed_id = PodcastFeedId::new();
        let url = url.to_string();
        let (tx, rx) = mpsc::channel();
        self.inflight = Some((tr_args!("Fetching {url}", "url" => &url), rx));
        self.status = tr_args!("Fetching {url}…", "url" => &url);
        std::thread::spawn(move || {
            let result = fetch_feed(&url);
            let _ = tx.send(Job::Refresh {
                feed_id,
                url: url.clone(),
                result,
            });
        });
        Ok(())
    }

    pub fn refresh_feed(&mut self, feed: &PodcastFeed) -> Result<(), String> {
        if self.is_busy() {
            return Err(tr!("Another podcast task is still running."));
        }
        let feed_id = feed.id;
        let url = feed.url.clone();
        let (tx, rx) = mpsc::channel();
        self.inflight = Some((tr_args!("Refreshing {title}", "title" => &feed.title), rx));
        self.status = tr_args!("Refreshing {title}…", "title" => &feed.title);
        std::thread::spawn(move || {
            let result = fetch_feed(&url);
            let _ = tx.send(Job::Refresh {
                feed_id,
                url: url.clone(),
                result,
            });
        });
        Ok(())
    }

    pub fn download_episode(&mut self, episode: &PodcastEpisode) -> Result<(), String> {
        if self.is_busy() {
            return Err(tr!("Another podcast task is still running."));
        }
        if episode.is_downloaded() {
            return Ok(());
        }
        let episode = episode.clone();
        let destination_dir = self.download_dir.clone();
        let file_name = episode_file_name(&episode);
        let (tx, rx) = mpsc::channel();
        self.inflight = Some((
            tr_args!("Downloading {title}", "title" => &episode.title),
            rx,
        ));
        self.status = tr_args!("Downloading {title}…", "title" => &episode.title);
        std::thread::spawn(move || {
            let destination = destination_dir.join(file_name);
            let result = download_file(&episode.enclosure_url, &destination).map(|_| destination);
            let _ = tx.send(Job::Download { episode, result });
        });
        Ok(())
    }

    /// Apply a finished job. Returns an event for the UI, or `None` while the
    /// worker is still running.
    pub fn poll(&mut self) -> Option<PodcastEvent> {
        let (_label, rx) = self.inflight.take()?;
        match rx.try_recv() {
            Ok(Job::Refresh {
                feed_id,
                url,
                result,
            }) => {
                self.status.clear();
                match result {
                    Ok(parsed) => match self.store_feed(feed_id, url, parsed) {
                        Ok(feed_id) => Some(PodcastEvent::FeedUpdated { feed_id }),
                        Err(e) => Some(PodcastEvent::Error(e)),
                    },
                    Err(e) => Some(PodcastEvent::Error(e)),
                }
            }
            Ok(Job::Download { episode, result }) => {
                self.status.clear();
                match result {
                    Ok(path) => match self.repo.set_downloaded(&episode.id, &path) {
                        Ok(()) => Some(PodcastEvent::Downloaded {
                            episode_id: episode.id,
                        }),
                        Err(e) => Some(PodcastEvent::Error(tr_args!(
                            "Could not record the downloaded episode: {error}",
                            "error" => e
                        ))),
                    },
                    Err(e) => Some(PodcastEvent::Error(e)),
                }
            }
            Err(TryRecvError::Empty) => {
                self.inflight = Some((_label, rx));
                None
            }
            Err(TryRecvError::Disconnected) => {
                self.status.clear();
                Some(PodcastEvent::Error(tr!(
                    "The podcast worker stopped unexpectedly."
                )))
            }
        }
    }

    fn store_feed(
        &self,
        feed_id: PodcastFeedId,
        url: String,
        parsed: ParsedFeed,
    ) -> Result<PodcastFeedId, String> {
        let now = unix_now();
        let existing = self.repo.get_feed(&feed_id).map_err(|e| e.to_string())?;
        let feed = PodcastFeed {
            id: feed_id,
            url: existing.as_ref().map(|f| f.url.clone()).unwrap_or(url),
            title: parsed.title,
            description: parsed.description,
            artwork_url: parsed.artwork_url,
            last_fetched_at: Some(now),
            added_at: existing.map(|f| f.added_at).unwrap_or(now),
        };
        let stored = self.repo.upsert_feed(&feed).map_err(|e| e.to_string())?;

        for episode in parsed.episodes {
            let model = PodcastEpisode {
                id: PodcastEpisodeId::new(),
                feed_id: stored.id,
                guid: episode.guid,
                title: episode.title,
                description: episode.description,
                enclosure_url: episode.enclosure_url,
                published_at: episode.published_at,
                duration_secs: episode.duration_secs,
                local_path: None,
                position_secs: 0.0,
                completed: false,
                added_at: now,
            };
            self.repo
                .upsert_episode(&model)
                .map_err(|e| e.to_string())?;
        }
        Ok(stored.id)
    }
}

fn episode_file_name(episode: &PodcastEpisode) -> String {
    let extension = episode
        .enclosure_url
        .split(['?', '#'])
        .next()
        .and_then(|u| u.rsplit('.').next())
        .filter(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mp3" | "m4a" | "aac" | "ogg" | "oga" | "opus" | "wav" | "flac" | "mp4"
            )
        })
        .unwrap_or("mp3");
    format!("{}.{}", episode.id, extension)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(45))
        .user_agent(USER_AGENT)
        .build()
}

fn fetch_feed(url: &str) -> Result<ParsedFeed, String> {
    let response = agent().get(url).call().map_err(|e| {
        tr_args!(
            "Could not fetch the feed: {error}",
            "error" => e
        )
    })?;
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_FEED_BYTES)
        .read_to_string(&mut body)
        .map_err(|e| {
            tr_args!(
                "Could not read the feed: {error}",
                "error" => e
            )
        })?;
    parse_feed(&body)
}

fn download_file(url: &str, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            tr_args!(
                "Could not create the podcast folder: {error}",
                "error" => e
            )
        })?;
    }
    let response = agent().get(url).call().map_err(|e| {
        tr_args!(
            "Could not download the episode: {error}",
            "error" => e
        )
    })?;
    let mut reader = response
        .into_reader()
        .take(MAX_EPISODE_BYTES.saturating_add(1));
    let temp = destination.with_extension("part");
    let written = {
        let mut file = std::fs::File::create(&temp).map_err(|e| {
            tr_args!(
                "Could not create the episode file: {error}",
                "error" => e
            )
        })?;
        std::io::copy(&mut reader, &mut file).map_err(|e| {
            tr_args!(
                "Could not write the episode file: {error}",
                "error" => e
            )
        })?
    };
    if written > MAX_EPISODE_BYTES {
        let _ = std::fs::remove_file(&temp);
        return Err(tr!("The episode is larger than the 512 MB download limit."));
    }
    std::fs::rename(&temp, destination).map_err(|e| {
        tr_args!(
            "Could not finish the episode download: {error}",
            "error" => e
        )
    })?;
    Ok(())
}

/// Parse RSS 2.0 or Atom. Namespace prefixes are ignored (the element's local
/// name is what is compared), which is how `itunes:duration` and
/// `content:encoded` are reached without a namespace table.
pub fn parse_feed(xml: &str) -> Result<ParsedFeed, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| {
        tr_args!(
            "Invalid feed XML: {error}",
            "error" => e
        )
    })?;
    let root = doc.root_element();

    let channel = root
        .children()
        .find(|n| n.is_element() && (n.tag_name().name() == "channel"))
        .unwrap_or(root);
    let is_atom = root.tag_name().name() == "feed";

    let title = child_text(channel, "title")
        .map(|t| clean_text(&t))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| tr!("Untitled podcast"));
    let description = child_text(channel, "description")
        .or_else(|| child_text(channel, "subtitle"))
        .map(|t| clean_text(&t))
        .filter(|t| !t.is_empty());

    let artwork_url = channel
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "image")
        .and_then(|image| {
            image
                .attribute("href")
                .map(str::to_string)
                .or_else(|| child_text(image, "url"))
        })
        .or_else(|| {
            channel
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "logo")
                .and_then(|n| n.text().map(str::to_string))
        });

    let item_name = if is_atom { "entry" } else { "item" };
    let mut episodes = Vec::new();
    for item in channel
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == item_name)
    {
        let Some(enclosure_url) = enclosure_url(item) else {
            continue;
        };
        let guid = child_text(item, "guid")
            .or_else(|| child_text(item, "id"))
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty())
            .unwrap_or_else(|| enclosure_url.clone());
        let title = child_text(item, "title")
            .map(|t| clean_text(&t))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| tr!("Untitled episode"));
        let description = child_text(item, "description")
            .or_else(|| child_text(item, "summary"))
            .or_else(|| child_text(item, "encoded"))
            .map(|t| clean_text(&t))
            .filter(|t| !t.is_empty());
        let published_at = child_text(item, "pubDate")
            .or_else(|| child_text(item, "published"))
            .or_else(|| child_text(item, "updated"))
            .and_then(|d| parse_date(&d));
        let duration_secs = child_text(item, "duration").and_then(|d| parse_duration(&d));

        episodes.push(ParsedEpisode {
            guid,
            title,
            description,
            enclosure_url,
            published_at,
            duration_secs,
        });
    }

    // A feed that repeats a guid must not produce two rows for one episode.
    let mut seen = std::collections::HashSet::new();
    episodes.retain(|e| seen.insert(e.guid.clone()));

    if episodes.is_empty() {
        return Err(tr!("The feed contains no playable episodes."));
    }

    Ok(ParsedFeed {
        title,
        description,
        artwork_url,
        episodes,
    })
}

fn child_text<'a, 'input>(node: roxmltree::Node<'a, 'input>, name: &str) -> Option<String> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .and_then(|n| n.text())
        .map(str::to_string)
}

fn enclosure_url(node: roxmltree::Node<'_, '_>) -> Option<String> {
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "enclosure" {
            if let Some(url) = child.attribute("url") {
                return Some(url.to_string());
            }
        }
        if child.tag_name().name() == "link" {
            let rel = child.attribute("rel").unwrap_or("alternate");
            if rel == "enclosure" {
                if let Some(href) = child.attribute("href") {
                    return Some(href.to_string());
                }
            }
        }
    }
    None
}

/// Strip the HTML that feed descriptions routinely contain and collapse
/// whitespace. A biography or episode summary is read, not rendered.
fn clean_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let decoded = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse `HH:MM:SS`, `MM:SS`, or a plain number of seconds.
fn parse_duration(raw: &str) -> Option<f32> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let parts: Vec<&str> = raw.split(':').collect();
    let seconds = match parts.as_slice() {
        [secs] => secs.parse::<f32>().ok()?,
        [mins, secs] => mins.parse::<f32>().ok()? * 60.0 + secs.parse::<f32>().ok()?,
        [hours, mins, secs] => {
            hours.parse::<f32>().ok()? * 3600.0
                + mins.parse::<f32>().ok()? * 60.0
                + secs.parse::<f32>().ok()?
        }
        _ => return None,
    };
    (seconds.is_finite() && seconds > 0.0).then_some(seconds)
}

/// Parse the two date forms feeds actually publish: RFC 2822
/// (`Wed, 01 Jan 2025 12:00:00 GMT`) and ISO 8601.
fn parse_date(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if raw.len() >= 10 && raw.as_bytes().get(4) == Some(&b'-') {
        let year: i64 = raw.get(0..4)?.parse().ok()?;
        let month: i64 = raw.get(5..7)?.parse().ok()?;
        let day: i64 = raw.get(8..10)?.parse().ok()?;
        let (hour, minute, second) = raw
            .get(11..19)
            .and_then(|t| {
                Some((
                    t.get(0..2)?.parse::<i64>().ok()?,
                    t.get(3..5)?.parse::<i64>().ok()?,
                    t.get(6..8)?.parse::<i64>().ok()?,
                ))
            })
            .unwrap_or((0, 0, 0));
        return Some(unix_seconds(year, month, day, hour, minute, second));
    }

    let parts: Vec<&str> = raw.split_whitespace().collect();
    // [Wed,] 01 Jan 2025 12:00:00 GMT
    let offset = if parts.first().is_some_and(|p| p.ends_with(',')) {
        1
    } else {
        0
    };
    let day: i64 = parts.get(offset)?.parse().ok()?;
    let month = month_number(parts.get(offset + 1)?)?;
    let year: i64 = parts.get(offset + 2)?.parse().ok()?;
    let time: Vec<&str> = parts.get(offset + 3)?.split(':').collect();
    let hour: i64 = time.first()?.parse().ok()?;
    let minute: i64 = time.get(1).and_then(|m| m.parse().ok()).unwrap_or(0);
    let second: i64 = time.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    Some(unix_seconds(year, month, day, hour, minute, second))
}

fn month_number(name: &str) -> Option<i64> {
    match name.get(..3)?.to_ascii_lowercase().as_str() {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

/// Days since 1970-01-01, from a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn unix_seconds(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> i64 {
    days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?>
    <rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
         xmlns:content="http://purl.org/rss/1.0/modules/content/">
      <channel>
        <title>The Example Show</title>
        <description>A show about &amp; examples.</description>
        <itunes:image href="https://example.com/cover.jpg"/>
        <item>
          <title>Episode One</title>
          <guid>ep-1</guid>
          <description>&lt;p&gt;The first one.&lt;/p&gt;</description>
          <enclosure url="https://example.com/1.mp3" type="audio/mpeg" length="10"/>
          <pubDate>Wed, 01 Jan 2025 12:00:00 GMT</pubDate>
          <itunes:duration>12:34</itunes:duration>
        </item>
        <item>
          <title>Episode Two</title>
          <guid>ep-2</guid>
          <enclosure url="https://example.com/2.m4a" type="audio/mp4" length="10"/>
          <itunes:duration>1:02:03</itunes:duration>
        </item>
        <item>
          <title>No audio</title>
          <guid>ep-3</guid>
        </item>
      </channel>
    </rss>"#;

    #[test]
    fn parses_rss_titles_descriptions_and_enclosures() {
        let feed = parse_feed(RSS).unwrap();
        assert_eq!(feed.title, "The Example Show");
        assert_eq!(
            feed.description.as_deref(),
            Some("A show about & examples.")
        );
        assert_eq!(
            feed.artwork_url.as_deref(),
            Some("https://example.com/cover.jpg")
        );
        assert_eq!(feed.episodes.len(), 2, "the item without audio is skipped");
        assert_eq!(feed.episodes[0].guid, "ep-1");
        assert_eq!(feed.episodes[0].duration_secs, Some(754.0));
        assert_eq!(feed.episodes[1].duration_secs, Some(3723.0));
        assert_eq!(feed.episodes[0].published_at, Some(1_735_732_800));
    }

    #[test]
    fn duplicates_are_collapsed_by_guid() {
        let duplicated = RSS.replace("<guid>ep-2</guid>", "<guid>ep-1</guid>");
        let feed = parse_feed(&duplicated).unwrap();
        assert_eq!(feed.episodes.len(), 1);
    }

    #[test]
    fn parses_atom_entries() {
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom">
          <title>Atom Show</title>
          <subtitle>Sub</subtitle>
          <entry>
            <id>tag:example,2025:1</id>
            <title>Atom Episode</title>
            <updated>2025-02-03T04:05:06Z</updated>
            <link rel="enclosure" href="https://example.com/a.ogg"/>
          </entry>
        </feed>"#;
        let feed = parse_feed(atom).unwrap();
        assert_eq!(feed.title, "Atom Show");
        assert_eq!(feed.episodes[0].enclosure_url, "https://example.com/a.ogg");
        assert_eq!(feed.episodes[0].guid, "tag:example,2025:1");
        assert!(feed.episodes[0].published_at.is_some());
    }

    #[test]
    fn rejects_xml_without_playable_episodes() {
        assert!(parse_feed("<rss><channel><title>x</title></channel></rss>").is_err());
        assert!(parse_feed("not xml").is_err());
    }

    #[test]
    fn duration_forms() {
        assert_eq!(parse_duration("90"), Some(90.0));
        assert_eq!(parse_duration("01:30"), Some(90.0));
        assert_eq!(parse_duration("1:00:01"), Some(3601.0));
        assert_eq!(parse_duration("0"), None);
    }
}
