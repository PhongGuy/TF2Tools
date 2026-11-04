use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tf2tools_app::audio::{
    AudioCall, AudioError, AudioOut, Output, Playback, RecordingAudio, RodioOut,
};

#[test]
fn the_recording_fake_records_and_ends_on_request() {
    let audio = RecordingAudio::new();
    let first = audio.play(vec![1, 2], 0.5);
    assert!(first.try_ended().is_none());
    audio.set_volume(0.25);
    let second = audio.play(vec![3], 0.75);
    assert_eq!(first.try_ended(), Some(Ok(())));
    audio.finish();
    assert_eq!(second.try_ended(), Some(Ok(())));
    audio.stop();
    assert_eq!(
        audio.calls(),
        vec![
            AudioCall::Play {
                wav: vec![1, 2],
                volume: 0.5
            },
            AudioCall::SetVolume(0.25),
            AudioCall::Play {
                wav: vec![3],
                volume: 0.75
            },
            AudioCall::Stop,
        ]
    );
    assert_eq!(audio.plays(), vec![(vec![1, 2], 0.5), (vec![3], 0.75)]);
}

#[test]
fn the_recording_fake_fails_a_scripted_play_once() {
    let audio = RecordingAudio::new();
    audio.fail_next(AudioError::Device("no device".into()));
    assert_eq!(
        audio.play(vec![1], 0.5).try_ended(),
        Some(Err(AudioError::Device("no device".into())))
    );
    assert!(audio.play(vec![1], 0.5).try_ended().is_none());
}

/// A TF2-playable WAV: 16-bit 44.1 kHz stereo PCM, `frames` frames of silence.
fn wav(frames: u32) -> Vec<u8> {
    let data_len = frames * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(out.len() + data_len as usize, 0);
    out
}

/// Ends a drain thread when the audio thread drops its output.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// A mixer nobody hears. With `drain`, a thread pulls samples far faster than real time,
/// so sounds end in milliseconds; without, nothing pulls and a sound never ends.
fn silent(drain: bool) -> Output {
    let (mixer, mut source) =
        rodio::mixer::mixer(NonZero::new(2).unwrap(), NonZero::new(44_100).unwrap());
    if !drain {
        return Output::new(mixer, source);
    }
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    std::thread::spawn(move || {
        while !stopped.load(Ordering::SeqCst) {
            for _ in 0..4096 {
                if source.next().is_none() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    Output::new(mixer, StopOnDrop(stop))
}

/// A `RodioOut` over silent outputs, and a count of how often it opened one.
fn out(drain: bool) -> (RodioOut, Arc<AtomicUsize>) {
    let opens = Arc::new(AtomicUsize::new(0));
    let counter = opens.clone();
    let out = RodioOut::with_output(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(silent(drain))
    });
    (out, opens)
}

/// The playback's outcome, waiting up to five seconds for it.
fn ended(playback: &Playback) -> Result<(), AudioError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(outcome) = playback.try_ended() {
            return outcome;
        }
        assert!(Instant::now() < deadline, "the playback never ended");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// True if the playback is still going 100 ms from now.
fn still_playing(playback: &Playback) -> bool {
    std::thread::sleep(Duration::from_millis(100));
    playback.try_ended().is_none()
}

#[test]
fn a_played_wav_ends_on_its_own() {
    let (out, opens) = out(true);
    let playback = out.play(wav(4_410), 0.5);
    assert_eq!(ended(&playback), Ok(()));
    assert_eq!(opens.load(Ordering::SeqCst), 1);
}

#[test]
fn the_device_opens_once_across_plays() {
    let (out, opens) = out(true);
    assert_eq!(ended(&out.play(wav(441), 0.5)), Ok(()));
    assert_eq!(ended(&out.play(wav(441), 0.5)), Ok(()));
    assert_eq!(opens.load(Ordering::SeqCst), 1);
}

#[test]
fn garbage_fails_to_decode_before_any_device_opens() {
    let (out, opens) = out(true);
    let playback = out.play(b"not a wav".to_vec(), 0.5);
    assert!(matches!(ended(&playback), Err(AudioError::Decode(_))));
    assert_eq!(opens.load(Ordering::SeqCst), 0);
}

#[test]
fn a_failed_device_is_retried_by_the_next_play() {
    let opens = Arc::new(AtomicUsize::new(0));
    let counter = opens.clone();
    let out = RodioOut::with_output(move || {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(AudioError::Device("no device".into()))
        } else {
            Ok(silent(true))
        }
    });
    assert_eq!(
        ended(&out.play(wav(441), 0.5)),
        Err(AudioError::Device("no device".into()))
    );
    assert_eq!(ended(&out.play(wav(441), 0.5)), Ok(()));
    assert_eq!(opens.load(Ordering::SeqCst), 2);
}

#[test]
fn stop_ends_the_playing_sound() {
    let (out, _) = out(false);
    let playback = out.play(wav(44_100), 0.5);
    assert!(still_playing(&playback));
    out.stop();
    assert_eq!(ended(&playback), Ok(()));
}

#[test]
fn a_new_play_replaces_the_old_one() {
    let (out, _) = out(false);
    let first = out.play(wav(44_100), 0.5);
    let second = out.play(wav(44_100), 0.5);
    assert_eq!(ended(&first), Ok(()));
    assert!(still_playing(&second));
}

#[test]
fn a_volume_change_keeps_the_sound_playing() {
    let (out, _) = out(false);
    let playback = out.play(wav(44_100), 0.5);
    out.set_volume(0.1);
    assert!(still_playing(&playback));
}

#[test]
fn a_played_mp3_preview_ends_on_its_own() {
    let (out, _) = out(true);
    let mp3 = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/preview.mp3"
    ))
    .unwrap();
    let playback = out.play(mp3, 0.5);
    assert_eq!(ended(&playback), Ok(()));
}
