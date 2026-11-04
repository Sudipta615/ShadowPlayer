//! Audio-CD extraction through `libcdio`, `libcdio-paranoia` and (optionally)
//! `libcddb`.
//!
//! Drives are opened by path, so the same code works for `/dev/sr0`, a macOS
//! device node, and a Windows drive letter. Every native library is loaded at
//! runtime: a system without CD support still builds and still plays files.
//!
//! Extraction is track-at-a-time and checks the sector count the drive reports.
//! Output is a 16-bit 44.1 kHz WAV, optionally encoded through
//! [`crate::transcode`]. CDDB is best-effort; a failed lookup never loses audio.

pub use rip::*;
pub use types::*;

mod rip;
mod types;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_are_safe_on_every_platform() {
        assert_eq!(sanitise_filename("AC/DC: Live?"), "AC_DC_ Live_");
        assert_eq!(sanitise_filename("  ..  "), "Untitled");
        assert_eq!(sanitise_filename("CON"), "CON_");
        assert_eq!(sanitise_filename("nul"), "nul_");
        assert_eq!(sanitise_filename("line\nbreak"), "line_break");
    }

    #[test]
    fn track_names_are_numbered_under_the_album() {
        let disc = CdDisc {
            device: "/dev/sr0".to_string(),
            tracks: Vec::new(),
            metadata: DiscMetadata {
                title: Some("An Album".to_string()),
                ..Default::default()
            },
            warnings: Vec::new(),
        };
        let track = CdTrack {
            number: 3,
            title: "A Song".to_string(),
            artist: String::new(),
            first_sector: 0,
            last_sector: 100,
            duration_secs: 1.0,
        };
        assert_eq!(track_file_stem(&disc, &track), "An Album/03 - A Song");
    }

    #[test]
    fn the_wav_header_describes_the_audio_it_carries() {
        let dir = std::env::temp_dir().join(format!(
            "sp_cd_wav_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("track.wav");
        write_wav_i16(&path, 44_100, 2, &[0, i16::MAX, i16::MIN, 0]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            44_100
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_discs_duration_is_the_sum_of_its_tracks() {
        let disc = CdDisc {
            device: "x".into(),
            tracks: vec![
                CdTrack {
                    number: 1,
                    title: "a".into(),
                    artist: "".into(),
                    first_sector: 0,
                    last_sector: 74,
                    duration_secs: 1.0,
                },
                CdTrack {
                    number: 2,
                    title: "b".into(),
                    artist: "".into(),
                    first_sector: 75,
                    last_sector: 149,
                    duration_secs: 1.0,
                },
            ],
            metadata: DiscMetadata::default(),
            warnings: Vec::new(),
        };
        assert!((disc.total_duration_secs() - 2.0).abs() < f64::EPSILON);
    }
}
