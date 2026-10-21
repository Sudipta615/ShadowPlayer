//! Loudness tag write-back: what goes in must come out.
//!
//! The engine can write EBU R128 and ReplayGain 2.0 values back into a
//! file's tags (behind the `tag-write` feature) and read them on the next
//! load. This suite checks that the round trip is lossless for the values the
//! engine claims to write.
//!
//! The container matters: the R128 keys are Vorbis-comment keys, and lofty
//! 0.25 no longer exposes arbitrary vendor keys, so the R128 gain is only
//! written into a Vorbis-comment container (FLAC / Ogg / Opus). The fixture
//! here is therefore a minimal Ogg Opus stream — an `OpusHead` packet and an
//! `OpusTags` packet, built here so no binary asset is committed — because
//! that is the container in which both the R128 and the ReplayGain values
//! survive the trip.
//!
//! One documented exception: the **R128 peak** is not written. Lofty has no
//! `R128_TRACK_PEAK` key, so writing one would produce a tag no reader
//! understands; the peak stays in the engine's own cache instead. The suite
//! asserts that asymmetry rather than pretending it round trips.

mod common;

#[cfg(feature = "tag-write")]
mod with_tag_write {
    use crate::common;
    use engine::decode::write_loudness_tags;
    use engine::dsp::LoudnessMetadata;

    /// The Ogg CRC-32: polynomial `0x04c11db7`, no reflection, no final XOR.
    fn ogg_crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0;
        for &byte in data {
            crc ^= (byte as u32) << 24;
            for _ in 0..8 {
                if crc & 0x8000_0000 != 0 {
                    crc = (crc << 1) ^ 0x04c1_1db7;
                } else {
                    crc <<= 1;
                }
            }
        }
        crc
    }

    /// Build one Ogg page holding `packets`.
    fn ogg_page(
        header_type: u8,
        granule: u64,
        serial: u32,
        sequence: u32,
        packets: &[&[u8]],
    ) -> Vec<u8> {
        // The segment table: a packet of N bytes is N/255 full segments plus
        // one final segment holding the remainder.
        let mut segments: Vec<u8> = Vec::new();
        let mut body: Vec<u8> = Vec::new();
        for packet in packets {
            let mut remaining = packet.len();
            while remaining >= 255 {
                segments.push(255);
                remaining -= 255;
            }
            segments.push(remaining as u8);
            body.extend_from_slice(packet);
        }

        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0); // stream structure version
        page.push(header_type);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&serial.to_le_bytes());
        page.extend_from_slice(&sequence.to_le_bytes());
        let crc_position = page.len();
        page.extend_from_slice(&0u32.to_le_bytes()); // CRC placeholder
        page.push(segments.len() as u8);
        page.extend_from_slice(&segments);
        page.extend_from_slice(&body);

        let crc = ogg_crc32(&page);
        page[crc_position..crc_position + 4].copy_from_slice(&crc.to_le_bytes());
        page
    }

    /// The `OpusHead` identification packet.
    fn opus_head() -> Vec<u8> {
        let mut head = Vec::new();
        head.extend_from_slice(b"OpusHead");
        head.push(1); // version
        head.push(2); // channel count
        head.extend_from_slice(&312u16.to_le_bytes()); // pre-skip
        head.extend_from_slice(&48_000u32.to_le_bytes()); // original sample rate
        head.extend_from_slice(&0u16.to_le_bytes()); // output gain
        head.push(0); // channel mapping family
        head
    }

    /// An `OpusTags` packet holding the given Vorbis comments.
    fn opus_tags(comments: &[&str]) -> Vec<u8> {
        let mut tags = Vec::new();
        tags.extend_from_slice(b"OpusTags");
        let vendor = b"engine-test";
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for comment in comments {
            tags.extend_from_slice(&(comment.len() as u32).to_le_bytes());
            tags.extend_from_slice(comment.as_bytes());
        }
        tags
    }

    /// A minimal but valid Ogg Opus stream carrying the given comments.
    fn opus_fixture(comments: &[&str]) -> Vec<u8> {
        let head = opus_head();
        let tags = opus_tags(comments);
        let mut file = Vec::new();
        file.extend_from_slice(&ogg_page(0x02, 0, 0x1234_5678, 0, &[&head]));
        file.extend_from_slice(&ogg_page(0x00, 0, 0x1234_5678, 1, &[&tags]));
        file
    }

    /// Write an Opus fixture to a temporary file and return its path.
    fn write_opus_fixture(comments: &[&str]) -> common::TempFile {
        let bytes = opus_fixture(comments);
        common::write_bytes("tag_roundtrip", &bytes, "opus")
    }

    /// The values the suite writes and expects back.
    fn metadata() -> LoudnessMetadata {
        LoudnessMetadata {
            ebu_r128_loudness: Some(-14.25),
            ebu_r128_peak: Some(-1.5),
            replaygain_track_db: Some(-3.75),
            replaygain_track_peak: Some(0.5),
            replaygain_album_db: Some(-4.5),
            replaygain_album_peak: Some(0.6),
        }
    }

    #[test]
    fn the_replaygain_values_survive_the_round_trip_unchanged() {
        let file = write_opus_fixture(&["TITLE=Test"]);
        let written = metadata();
        write_loudness_tags(file.path(), &written).expect("the tags must be writable");

        let read = engine::decode::extract_loudness_metadata(file.path());
        assert_eq!(
            read.replaygain_track_db, written.replaygain_track_db,
            "the ReplayGain track gain must come back unchanged"
        );
        assert_eq!(
            read.replaygain_track_peak, written.replaygain_track_peak,
            "the ReplayGain track peak must come back unchanged"
        );
        assert_eq!(
            read.replaygain_album_db, written.replaygain_album_db,
            "the ReplayGain album gain must come back unchanged"
        );
        assert_eq!(
            read.replaygain_album_peak, written.replaygain_album_peak,
            "the ReplayGain album peak must come back unchanged"
        );
    }

    #[test]
    fn the_r128_loudness_survives_the_round_trip_unchanged() {
        let file = write_opus_fixture(&["TITLE=Test"]);
        let written = metadata();
        write_loudness_tags(file.path(), &written).expect("the tags must be writable");

        let read = engine::decode::extract_loudness_metadata(file.path());
        // The R128 gain is written in the EBU integer form (LUFS × 100), so
        // the round trip is exact to a hundredth of a LU.
        assert_eq!(
            read.ebu_r128_loudness, written.ebu_r128_loudness,
            "the R128 integrated loudness must come back unchanged"
        );
    }

    #[test]
    fn the_r128_peak_is_not_written_because_no_reader_would_understand_it() {
        // Lofty has no `R128_TRACK_PEAK` key. Writing one would put a tag in
        // the file that nothing reads, so the engine keeps the peak in its
        // own cache and does not pretend otherwise.
        let file = write_opus_fixture(&["TITLE=Test"]);
        let written = metadata();
        write_loudness_tags(file.path(), &written).expect("the tags must be writable");

        let read = engine::decode::extract_loudness_metadata(file.path());
        assert_eq!(
            read.ebu_r128_peak, None,
            "the R128 peak must not be written into the file"
        );
        assert!(
            read.ebu_r128_loudness.is_some(),
            "the rest of the metadata must still have been written"
        );
    }

    #[test]
    fn a_file_with_no_loudness_tags_reads_back_as_no_metadata() {
        let file = write_opus_fixture(&["TITLE=Nothing"]);
        let read = engine::decode::extract_loudness_metadata(file.path());
        assert_eq!(read.replaygain_track_db, None);
        assert_eq!(read.replaygain_track_peak, None);
        assert_eq!(read.ebu_r128_loudness, None);
    }

    #[test]
    fn writing_nothing_is_a_no_op_rather_than_an_empty_tag() {
        let file = write_opus_fixture(&["TITLE=Test"]);
        let before = std::fs::read(file.path()).expect("the fixture must be readable");
        write_loudness_tags(file.path(), &LoudnessMetadata::default())
            .expect("writing nothing must not fail");
        let after = std::fs::read(file.path()).expect("the fixture must be readable");
        assert_eq!(
            before, after,
            "a metadata record with no values must leave the file alone"
        );
    }

    #[test]
    fn a_container_that_cannot_carry_tags_is_refused_by_name() {
        // Raw PCM and DSD have nowhere to put the values, and a DSF's
        // block-aligned layout makes mutating its ID3v2 tag risky.
        let pcm = common::write_bytes("tag_roundtrip", b"not audio", "pcm");
        let error = write_loudness_tags(pcm.path(), &metadata())
            .expect_err("a raw PCM file must be refused");
        assert!(
            matches!(
                error,
                engine::decode::TagWriteError::UnsupportedContainer(_)
            ),
            "a raw PCM file must be refused with UnsupportedContainer, got {error:?}"
        );

        let dsf = common::write_bytes("tag_roundtrip", b"not audio", "dsf");
        let error =
            write_loudness_tags(dsf.path(), &metadata()).expect_err("a DSF file must be refused");
        assert!(
            matches!(
                error,
                engine::decode::TagWriteError::UnsupportedContainer(_)
            ),
            "a DSF file must be refused with UnsupportedContainer, got {error:?}"
        );
        assert!(
            error.to_string().contains("DSD"),
            "the refusal must name the format, it said: {error}"
        );
    }

    #[test]
    fn the_written_values_are_the_ones_a_tag_inspector_would_see() {
        // The written tag must carry the standard spellings, not the
        // engine's internal ones.
        let file = write_opus_fixture(&["TITLE=Test"]);
        write_loudness_tags(file.path(), &metadata()).expect("the tags must be writable");

        let bytes = std::fs::read(file.path()).expect("the fixture must be readable");
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("REPLAYGAIN_TRACK_GAIN"),
            "the standard ReplayGain key must be in the file"
        );
        assert!(
            text.contains("-3.75 dB"),
            "the ReplayGain gain must be written in the standard 'X.XX dB' form"
        );
        assert!(
            text.contains("R128_TRACK_GAIN"),
            "the standard R128 key must be in the file"
        );
        assert!(
            text.contains("-1425"),
            "the R128 gain must be written in the EBU integer form (LUFS × 100)"
        );
    }
}

#[cfg(not(feature = "tag-write"))]
mod without_tag_write {
    #[test]
    fn the_loudness_reader_reports_no_metadata_for_an_untagged_file() {
        // The *read* side of the loudness metadata is always available; only
        // the write-back needs the `tag-write` feature. This suite's
        // round-trip half is therefore compiled out without the feature, and
        // what remains is the reader's contract.
        let samples: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let interleaved: Vec<f32> = samples.iter().flat_map(|s| [*s, *s]).collect();
        let file = crate::common::write_wav(
            "tag_roundtrip",
            &interleaved,
            crate::common::CHANNELS,
            crate::common::SAMPLE_RATE,
        );

        let read = engine::decode::extract_loudness_metadata(file.path());
        assert_eq!(read.replaygain_track_db, None);
        assert_eq!(read.replaygain_track_peak, None);
        assert_eq!(read.ebu_r128_loudness, None);
        assert_eq!(read.ebu_r128_peak, None);
    }
}
