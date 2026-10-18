#[cfg(test)]
mod tests {
    use crate::parser::*;
    use std::fs;

    #[test]
    fn test_parse_lrc_basic() {
        let lrc = r#"
[ti:Midnight Dreams]
[ar:Horizon Lines]
[00:12.50]Dancing in the neon light
[00:18.00]Shadows whispering through the night
[00:25.30]Midnight dreams take flight
"#;

        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].start_time_secs, 12.5);
        assert_eq!(lines[0].text, "Dancing in the neon light");
        assert_eq!(lines[1].start_time_secs, 18.0);
        assert_eq!(lines[2].start_time_secs, 25.3);
    }

    #[test]
    fn test_find_active_line() {
        let lrc = r#"
[00:10.00]First line
[00:20.00]Second line
[00:30.00]Third line
"#;
        let lines = parse_lrc(lrc);

        assert_eq!(find_active_line(&lines, 5.0), None);
        assert_eq!(find_active_line(&lines, 10.0), Some(0));
        assert_eq!(find_active_line(&lines, 15.0), Some(0));
        assert_eq!(find_active_line(&lines, 20.0), Some(1));
        assert_eq!(find_active_line(&lines, 25.0), Some(1));
        assert_eq!(find_active_line(&lines, 30.0), Some(2));
        assert_eq!(find_active_line(&lines, 99.0), Some(2));
    }

    #[test]
    fn test_interpret_lyrics_text() {
        let synced = interpret_lyrics_text("[00:01.00]Hello\n[00:02.00]World");
        assert!(synced.0);
        assert_eq!(synced.1.len(), 2);

        // Prose without timestamps is plain, not synced.
        let plain = interpret_lyrics_text("Hello\nWorld");
        assert!(!plain.0);
        assert!(plain.1.is_empty());
    }

    /// Create an isolated scratch directory for a test.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sp_lyrics_{}_{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_sidecar_matches_stem_and_extension_case_insensitively() {
        let dir = scratch("case");
        let song = dir.join("Song.FLAC");
        fs::write(&song, b"not really audio").unwrap();
        fs::write(dir.join("sOnG.LRC"), "[00:05.00]Found me").unwrap();

        let (is_synced, lines, raw) = find_sidecar_lyrics(&song).expect("sidecar should be found");
        assert!(is_synced);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].start_time_secs, 5.0);
        assert!(raw.unwrap().contains("Found me"));
    }

    #[test]
    fn test_lrc_sidecar_wins_over_txt() {
        let dir = scratch("prefer_lrc");
        let song = dir.join("track.flac");
        fs::write(&song, b"x").unwrap();
        fs::write(dir.join("track.txt"), "plain text only").unwrap();
        fs::write(dir.join("track.lrc"), "[00:01.00]synced wins").unwrap();

        let (is_synced, lines, _) = find_sidecar_lyrics(&song).unwrap();
        assert!(is_synced);
        assert_eq!(lines[0].text, "synced wins");
    }

    #[test]
    fn test_txt_sidecar_without_timestamps_is_plain_lyrics() {
        let dir = scratch("plain_txt");
        let song = dir.join("track.flac");
        fs::write(&song, b"x").unwrap();
        fs::write(dir.join("track.txt"), "Just\nsome\nprose").unwrap();

        let (is_synced, lines, raw) = find_sidecar_lyrics(&song).unwrap();
        assert!(!is_synced);
        assert!(lines.is_empty());
        assert_eq!(raw.unwrap(), "Just\nsome\nprose");
    }

    #[test]
    fn test_no_sidecar_returns_none() {
        let dir = scratch("absent");
        let song = dir.join("lonely.flac");
        fs::write(&song, b"x").unwrap();
        assert!(find_sidecar_lyrics(&song).is_none());
    }
}
