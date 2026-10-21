//! DSD: the container round trip and the DoP framing.
//!
//! Two separate things are checked here:
//!
//! * **DSF / DFF containers.** The reader must give back exactly the payload
//!   that was written, per channel, in the layout the format documents
//!   (blocks of `block_size` bytes per channel, the channels stored
//!   consecutively inside each block).
//! * **DoP (DSD over PCM) framing.** The engine documents the framing as
//!   `[23..16: marker][15..8: DSD byte 0][7..0: DSD byte 1]`, left-aligned in
//!   an `i32`, with the marker alternating `0x05` / `0xFA` per frame and both
//!   channels of a frame sharing the marker. The payload's own upper byte is
//!   substituted (`0x05` → `0x06`, `0xFA` → `0xFB`) so the data can never be
//!   mistaken for the sync pattern.
//!
//! The native DSD wire packing (the ALSA `SND_PCM_FORMAT_DSD_U8/U16/U32`
//! family) is checked for the same property: pack, unpack, get the bytes
//! back.

mod common;

use engine::decode::dsd::{DopPacker, DsdRate, DsdReader, DsdWireFormat, NativeDsdPacker};

/// The DSD64 bit rate.
const DSD64_HZ: u32 = 2_822_400;

/// Build a DSF container around per-channel payload byte planes.
///
/// The DSF layout is: a 28-byte `DSD ` header, a 52-byte `fmt ` chunk and a
/// `data` chunk. The audio is stored in blocks of `block_size` bytes per
/// channel, with the channels stored consecutively inside each block.
fn build_dsf(
    channels: usize,
    block_size: u32,
    bits_per_sample: u32,
    rate_hz: u32,
    planes: &[Vec<u8>],
) -> Vec<u8> {
    assert_eq!(planes.len(), channels);
    let block = block_size as usize;
    let padded: Vec<usize> = planes
        .iter()
        .map(|p| p.len().div_ceil(block) * block)
        .collect();
    let frames_per_channel = padded[0];

    let mut out = Vec::new();
    out.extend_from_slice(b"DSD ");
    out.extend_from_slice(&28u64.to_le_bytes());
    let total_size_pos = out.len();
    out.extend_from_slice(&0u64.to_le_bytes()); // total file size, patched below
    out.extend_from_slice(&0u64.to_le_bytes()); // metadata pointer

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&52u64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // format version
    out.extend_from_slice(&0u32.to_le_bytes()); // format id (DSD raw)
    out.extend_from_slice(&(channels as u32).to_le_bytes()); // channel type
    out.extend_from_slice(&(channels as u32).to_le_bytes()); // channel count
    out.extend_from_slice(&rate_hz.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(&((frames_per_channel * 8) as u64).to_le_bytes());
    out.extend_from_slice(&block_size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved

    // Interleave per block: [ch0 block][ch1 block]…
    let mut audio = vec![0u8; frames_per_channel * channels];
    for (ch, plane) in planes.iter().enumerate() {
        for (b, chunk) in plane.chunks(block).enumerate() {
            let base = b * block * channels + ch * block;
            audio[base..base + chunk.len()].copy_from_slice(chunk);
        }
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&((audio.len() as u64) + 12).to_le_bytes());
    out.extend_from_slice(&audio);

    let total = out.len() as u64;
    out[total_size_pos..total_size_pos + 8].copy_from_slice(&total.to_le_bytes());
    out
}

/// Build a DFF (DSDIFF) container around per-channel payload byte planes.
///
/// The DFF layout is big-endian: a `FRM8` form, a `PROP` chunk carrying the
/// sampling frequency, the channel count and the compression type, and a
/// `DSD ` chunk with the audio. The block size is fixed at 4096 bytes per
/// channel by the format, so a short payload is one partial block.
fn build_dff(rate_hz: u32, channels: u16, planes: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"FRM8");
    let total_size_pos = out.len();
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(b"DSD ");

    // PROP -> SND -> FS / CHNL / CMPR
    let mut prop = Vec::new();
    prop.extend_from_slice(b"SND ");
    prop.extend_from_slice(b"FS");
    prop.extend_from_slice(&4u64.to_be_bytes());
    prop.extend_from_slice(&rate_hz.to_be_bytes());
    prop.extend_from_slice(b"CHNL");
    prop.extend_from_slice(&2u64.to_be_bytes());
    prop.extend_from_slice(&channels.to_be_bytes());
    prop.extend_from_slice(b"CMPR");
    prop.extend_from_slice(&4u64.to_be_bytes());
    prop.extend_from_slice(b"DSD ");
    out.extend_from_slice(b"PROP");
    out.extend_from_slice(&(prop.len() as u64).to_be_bytes());
    out.extend_from_slice(&prop);

    // One partial block: the channels are stored consecutively.
    let mut audio = Vec::new();
    for plane in planes {
        audio.extend_from_slice(plane);
    }
    out.extend_from_slice(b"DSD ");
    out.extend_from_slice(&(audio.len() as u64).to_be_bytes());
    out.extend_from_slice(&audio);

    let total = out.len() as u64;
    out[total_size_pos..total_size_pos + 8].copy_from_slice(&(total - 12).to_be_bytes());
    out
}

/// A deterministic payload plane.
fn plane(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| ((i as u32 * 37 + seed as u32) & 0xFF) as u8)
        .collect()
}

/// Unpack a packed native-DSD byte stream back into per-channel byte planes.
fn unpack_native(
    format: DsdWireFormat,
    packed: &[u8],
    channels: usize,
    words: usize,
) -> Vec<Vec<u8>> {
    let bpw = format.bytes_per_word();
    let mut planes = vec![vec![0u8; words * bpw]; channels];
    for w in 0..words {
        for (ch, plane) in planes.iter_mut().enumerate() {
            let base = (w * channels + ch) * bpw;
            let src = &packed[base..base + bpw];
            match format {
                DsdWireFormat::U8 => plane[w] = src[0],
                DsdWireFormat::U16Le => {
                    plane[w * 2] = src[0];
                    plane[w * 2 + 1] = src[1];
                }
                DsdWireFormat::U16Be => {
                    plane[w * 2] = src[1];
                    plane[w * 2 + 1] = src[0];
                }
                DsdWireFormat::U32Le => plane[w * bpw..w * bpw + 4].copy_from_slice(src),
                DsdWireFormat::U32Be => {
                    plane[w * bpw..w * bpw + 4].copy_from_slice(&[src[3], src[2], src[1], src[0]]);
                }
            }
        }
    }
    planes
}

#[test]
fn a_dsf_file_gives_back_exactly_the_payload_that_was_written() {
    let ch0 = plane(0x11, 24);
    let ch1 = plane(0x77, 24);
    let bytes = build_dsf(2, 8, 1, DSD64_HZ, &[ch0.clone(), ch1.clone()]);
    let file = common::write_bytes("dsf", &bytes, "dsf");

    let mut reader = DsdReader::open(file.path()).expect("a well-formed DSF must open");
    assert_eq!(reader.rate(), DsdRate::Dsd64);
    assert_eq!(reader.channels(), 2);
    assert!(
        reader.is_lsb_first(),
        "a DSF with one bit per sample is LSB-first"
    );

    let mut got0 = Vec::new();
    let mut got1 = Vec::new();
    while let Some(block) = reader.read_dsd_block(1024).expect("the DSF must read") {
        got0.extend_from_slice(block.left());
        got1.extend_from_slice(block.right().expect("a stereo DSF has a right channel"));
    }
    assert_eq!(got0, ch0, "the left channel must come back byte for byte");
    assert_eq!(got1, ch1, "the right channel must come back byte for byte");
}

#[test]
fn a_dff_file_gives_back_exactly_the_payload_that_was_written() {
    let ch0 = plane(0x22, 200);
    let ch1 = plane(0x88, 200);
    let bytes = build_dff(DSD64_HZ, 2, &[ch0.clone(), ch1.clone()]);
    let file = common::write_bytes("dff", &bytes, "dff");

    let mut reader = DsdReader::open(file.path()).expect("a well-formed DFF must open");
    assert_eq!(reader.rate(), DsdRate::Dsd64);
    assert_eq!(reader.channels(), 2);
    assert!(reader.is_lsb_first(), "the DFF format is always LSB-first");

    let mut got0 = Vec::new();
    let mut got1 = Vec::new();
    while let Some(block) = reader.read_dsd_block(1024).expect("the DFF must read") {
        got0.extend_from_slice(block.left());
        got1.extend_from_slice(block.right().expect("a stereo DFF has a right channel"));
    }
    assert_eq!(got0, ch0, "the left channel must come back byte for byte");
    assert_eq!(got1, ch1, "the right channel must come back byte for byte");
}

#[test]
fn the_dsd_rates_round_trip_through_their_bit_rate() {
    for rate in [
        DsdRate::Dsd64,
        DsdRate::Dsd128,
        DsdRate::Dsd256,
        DsdRate::Dsd512,
        DsdRate::Dsd1024,
    ] {
        let hz = rate.sample_rate_hz();
        assert_eq!(
            DsdRate::from_hz(hz),
            Some(rate),
            "{rate:?} must come back from its own bit rate"
        );
    }
    assert_eq!(DsdRate::Dsd64.sample_rate_hz(), 2_822_400);
    assert_eq!(DsdRate::Dsd1024.sample_rate_hz(), 45_158_400);
    assert_eq!(
        DsdRate::from_hz(1_000_000),
        None,
        "a rate that is not a DSD multiple must not be accepted"
    );
}

#[test]
fn the_native_wire_packing_round_trips_for_every_format() {
    let bytes_per_channel = 8usize;
    let ch0 = plane(0x01, bytes_per_channel);
    let ch1 = plane(0x02, bytes_per_channel);
    let planes: Vec<&[u8]> = vec![&ch0, &ch1];

    for format in [
        DsdWireFormat::U8,
        DsdWireFormat::U16Le,
        DsdWireFormat::U16Be,
        DsdWireFormat::U32Le,
        DsdWireFormat::U32Be,
    ] {
        // A word is 8, 16 or 32 DSD samples depending on its width, so eight
        // payload bytes are one word in the U8 format and four words in the
        // U16 formats.
        let bpw = format.bytes_per_word();
        assert_eq!(format.samples_per_word(), bpw * 8);
        let words = bytes_per_channel / bpw;
        assert!(
            words > 0,
            "{format:?}: the payload must hold at least one whole word"
        );

        let mut packed = Vec::new();
        let count = NativeDsdPacker::pack(format, &planes, &mut packed);
        assert_eq!(
            count, words,
            "{format:?} must report one word per whole input word"
        );
        assert_eq!(
            packed.len(),
            words * 2 * bpw,
            "{format:?} must emit one word per channel per word index"
        );

        let unpacked = unpack_native(format, &packed, 2, words);
        assert_eq!(
            unpacked[0], ch0,
            "{format:?}: the left channel must round trip"
        );
        assert_eq!(
            unpacked[1], ch1,
            "{format:?}: the right channel must round trip"
        );
    }
}

#[test]
fn the_native_wire_formats_differ_in_their_byte_order() {
    let words = 4usize;
    let ch0 = plane(0x03, words);
    let ch1 = plane(0x04, words);
    let planes: Vec<&[u8]> = vec![&ch0, &ch1];

    let mut le = Vec::new();
    NativeDsdPacker::pack(DsdWireFormat::U16Le, &planes, &mut le);
    let mut be = Vec::new();
    NativeDsdPacker::pack(DsdWireFormat::U16Be, &planes, &mut be);
    assert_ne!(
        le, be,
        "the two byte orders must not produce the same bytes"
    );

    let mut le32 = Vec::new();
    NativeDsdPacker::pack(DsdWireFormat::U32Le, &planes, &mut le32);
    let mut be32 = Vec::new();
    NativeDsdPacker::pack(DsdWireFormat::U32Be, &planes, &mut be32);
    assert_ne!(
        le32, be32,
        "the two byte orders must not produce the same bytes"
    );

    // And the byte order really is reversed: the first two bytes of the
    // little-endian word are the low byte first.
    assert_eq!(le[0], ch0[0]);
    assert_eq!(le[1], ch0[1]);
    assert_eq!(be[0], ch0[1]);
    assert_eq!(be[1], ch0[0]);
}

#[test]
fn the_native_wire_frame_rate_is_the_bit_rate_divided_by_the_word_width() {
    // A DAC must be clocked at the rate the word format implies: DSD64 over
    // DSD_U8 runs at 352.8 kHz, over DSD_U16 at 176.4 kHz and over DSD_U32
    // at 88.2 kHz.
    assert_eq!(DsdWireFormat::U8.frame_rate_hz(DSD64_HZ), 352_800);
    assert_eq!(DsdWireFormat::U16Le.frame_rate_hz(DSD64_HZ), 176_400);
    assert_eq!(DsdWireFormat::U16Be.frame_rate_hz(DSD64_HZ), 176_400);
    assert_eq!(DsdWireFormat::U32Le.frame_rate_hz(DSD64_HZ), 88_200);
    assert_eq!(DsdWireFormat::U32Be.frame_rate_hz(DSD64_HZ), 88_200);
}

#[test]
fn the_dop_marker_alternates_between_the_two_documented_values() {
    let mut packer = DopPacker::new();
    let mut markers = Vec::new();
    for _ in 0..4 {
        let word = packer.pack_sample(0x1234);
        markers.push((word >> 24) & 0xFF);
    }
    assert_eq!(
        markers,
        vec![0x05, 0xFA, 0x05, 0xFA],
        "the DoP marker must alternate 0x05 / 0xFA"
    );
}

#[test]
fn the_dop_frame_carries_the_documented_word_layout() {
    // [23..16: marker][15..8: DSD byte 0][7..0: DSD byte 1], left-aligned in
    // an i32: the 24-bit word occupies bits 31..8.
    let mut packer = DopPacker::new();
    let word = packer.pack_sample(0xABCD);
    let marker = (word >> 24) & 0xFF;
    let payload = ((word as u32) >> 8) & 0xFFFF;
    assert_eq!(marker, 0x05);
    assert_eq!(payload, 0xABCD, "the 16-bit payload must survive verbatim");
    assert_eq!(
        word & 0xFF,
        0,
        "the word is left-aligned, so the low byte of the i32 is zero"
    );
}

#[test]
fn the_dop_marker_is_substituted_when_the_payload_would_look_like_one() {
    // The payload's own upper byte must never be mistaken for the sync
    // pattern, so 0x05 becomes 0x06 and 0xFA becomes 0xFB.
    let mut packer = DopPacker::new();
    assert_eq!((packer.pack_sample(0x0567) >> 24) & 0xFF, 0x06);
    assert_eq!((packer.pack_sample(0xFA67) >> 24) & 0xFF, 0xFB);
    assert_eq!((packer.pack_sample(0x0123) >> 24) & 0xFF, 0x05);
}

#[test]
fn both_channels_of_a_dop_frame_share_the_marker() {
    let mut packer = DopPacker::new();
    let (l0, r0) = packer.pack_stereo_frame(0x1111, 0x2222);
    let (l1, r1) = packer.pack_stereo_frame(0x3333, 0x4444);
    assert_eq!((l0 >> 24) & 0xFF, 0x05);
    assert_eq!((r0 >> 24) & 0xFF, 0x05);
    assert_eq!((l1 >> 24) & 0xFF, 0xFA);
    assert_eq!((r1 >> 24) & 0xFF, 0xFA);
    assert_eq!(((l0 as u32) >> 8) & 0xFFFF, 0x1111);
    assert_eq!(((r0 as u32) >> 8) & 0xFFFF, 0x2222);
    assert_eq!(((l1 as u32) >> 8) & 0xFFFF, 0x3333);
    assert_eq!(((r1 as u32) >> 8) & 0xFFFF, 0x4444);
}

#[test]
fn the_dop_substitution_is_evaluated_per_channel() {
    // The left channel's payload looks like a marker; the right channel's
    // does not. Each channel's substitution must be independent.
    let mut packer = DopPacker::new();
    let (left, right) = packer.pack_stereo_frame(0x0501, 0x1234);
    assert_eq!(
        (left >> 24) & 0xFF,
        0x06,
        "the left payload was substituted"
    );
    assert_eq!((right >> 24) & 0xFF, 0x05, "the right payload was not");
}

#[test]
fn the_dop_float_form_is_the_word_normalised_to_the_24_bit_range() {
    // Two packers, so the marker parity is the same in both runs.
    let mut ints = DopPacker::new();
    let (li, ri) = ints.pack_stereo_frame(0x1234, 0x5678);
    let mut floats = DopPacker::new();
    let (lf, rf) = floats.pack_stereo_frame_f32(0x1234, 0x5678);

    // The integer form is the 24-bit word left-aligned in an i32, so the
    // word itself is `packed >> 8` and the marker sits in bits 31..24 of
    // the i32.
    let word_l = li >> 8;
    let word_r = ri >> 8;
    assert_eq!((li >> 24) & 0xFF, 0x05, "the marker must be in the word");
    assert_eq!((ri >> 24) & 0xFF, 0x05);

    // The float form is `word / 2^23`, and a 24-bit word is exactly
    // representable in an f32, so the round trip through the float is exact.
    assert_eq!(
        (lf * 8_388_608.0) as i32,
        word_l,
        "the float form must be the word divided by 2^23"
    );
    assert_eq!((rf * 8_388_608.0) as i32, word_r);
    assert!(
        (-1.0..=1.0).contains(&lf) && (-1.0..=1.0).contains(&rf),
        "the float form must be normalised into [-1, 1]"
    );
}

#[test]
fn resetting_the_dop_packer_restarts_the_marker_parity() {
    // A seek restarts the frame parity; the DAC re-locks on the pattern.
    let mut packer = DopPacker::new();
    let _ = packer.pack_sample(0x1234);
    packer.reset();
    let word = packer.pack_sample(0x1234);
    assert_eq!(
        (word >> 24) & 0xFF,
        0x05,
        "after a reset the marker must start again at 0x05"
    );
}
