//! WAV header inspection: TF2's one accepted format, the rejected ones, and broken files.

mod common;

use common::wav_bytes;
use tf2tools_core::{WavError, inspect_wav};

#[test]
fn accepts_16_bit_44_1_khz_stereo_pcm() {
    let info = inspect_wav(&wav_bytes(1, 2, 44_100, 16, 400)).unwrap();
    assert_eq!(info.format_tag(), 1);
    assert_eq!(info.channels(), 2);
    assert_eq!(info.sample_rate(), 44_100);
    assert_eq!(info.bits_per_sample(), 16);
}

#[test]
fn rejects_every_other_format_and_reports_it() {
    for (tag, channels, rate, bits) in [
        (1, 1, 44_100, 16),
        (1, 2, 22_050, 16),
        (1, 2, 48_000, 16),
        (1, 2, 44_100, 8),
        (1, 2, 44_100, 24),
        (3, 2, 44_100, 32),
    ] {
        let err = inspect_wav(&wav_bytes(tag, channels, rate, bits, 64)).unwrap_err();
        let WavError::Unsupported { found } = err else {
            panic!("{tag}/{channels}/{rate}/{bits}: expected Unsupported, got {err:?}");
        };
        assert_eq!(
            (
                found.format_tag(),
                found.channels(),
                found.sample_rate(),
                found.bits_per_sample()
            ),
            (tag, channels, rate, bits)
        );
    }
}

#[test]
fn the_unsupported_message_names_what_was_found() {
    let err = inspect_wav(&wav_bytes(1, 1, 22_050, 16, 64)).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("1 channel(s)"), "{message}");
    assert!(message.contains("22050 Hz"), "{message}");
}

#[test]
fn chunks_before_fmt_are_skipped_with_padding() {
    let canonical = wav_bytes(1, 2, 44_100, 16, 8);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((canonical.len() - 8 + 14) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"LIST");
    bytes.extend_from_slice(&5u32.to_le_bytes());
    bytes.extend_from_slice(b"INFOx\0"); // odd size 5 plus one pad byte
    bytes.extend_from_slice(&canonical[12..]);
    assert_eq!(inspect_wav(&bytes).unwrap().channels(), 2);
}

#[test]
fn broken_files_are_malformed() {
    let good = wav_bytes(1, 2, 44_100, 16, 8);
    let mut not_riff = good.clone();
    not_riff[..4].copy_from_slice(b"RIFX");
    let mut not_wave = good.clone();
    not_wave[8..12].copy_from_slice(b"AVI ");
    let mut no_fmt = good[..12].to_vec();
    no_fmt.extend_from_slice(b"data\x04\0\0\0abcd");
    let cut_fmt = good[..30].to_vec();
    for (name, bytes) in [
        ("empty", Vec::new()),
        ("not riff", not_riff),
        ("not wave", not_wave),
        ("no fmt chunk", no_fmt),
        ("fmt cut short", cut_fmt),
    ] {
        let err = inspect_wav(&bytes).unwrap_err();
        assert!(matches!(err, WavError::Malformed { .. }), "{name}: {err:?}");
    }
}
