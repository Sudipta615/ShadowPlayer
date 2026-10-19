#[cfg(test)]
mod unit {
    use crate::ids::TrackId;
    use crate::models::{format_duration, AppSettings, SortField};
    use std::path::PathBuf;

    #[test]
    fn test_track_id_canonical() {
        let p1 = PathBuf::from("/music/song1.flac");
        let p2 = PathBuf::from("/music/song1.flac");
        let p3 = PathBuf::from("/music/song2.flac");

        let id1 = TrackId::from_canonical_path(&p1);
        let id2 = TrackId::from_canonical_path(&p2);
        let id3 = TrackId::from_canonical_path(&p3);

        assert_eq!(
            id1, id2,
            "Identical paths must produce identical stable IDs"
        );
        assert_ne!(id1, id3, "Different paths must produce distinct IDs");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(5.0), "0:05");
        assert_eq!(format_duration(65.0), "1:05");
        assert_eq!(format_duration(238.0), "3:58");
        assert_eq!(format_duration(3665.0), "1:01:05");
    }

    #[test]
    fn test_app_settings_default() {
        let s = AppSettings::default();
        assert!(s.theme_dark);
        assert_eq!(s.sort_field, SortField::Title);
        assert!((s.volume - 0.75).abs() < 1e-4);
    }
}
