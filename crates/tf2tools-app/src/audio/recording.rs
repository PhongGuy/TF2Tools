use std::sync::{Mutex, MutexGuard};

use super::{AudioError, AudioOut, Playback};

#[derive(Debug, Clone, PartialEq)]
pub enum AudioCall {
    Play { wav: Vec<u8>, volume: f32 },
    Stop,
    SetVolume(f32),
}

/// Records every call and plays nothing; a play lasts until `finish`, `stop` or the next play.
#[derive(Default)]
pub struct RecordingAudio {
    calls: Mutex<Vec<AudioCall>>,
    current: Mutex<Option<async_channel::Sender<AudioError>>>,
    fail: Mutex<Option<AudioError>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl RecordingAudio {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<AudioCall> {
        lock(&self.calls).clone()
    }

    /// The bytes and volume of every play, in order.
    pub fn plays(&self) -> Vec<(Vec<u8>, f32)> {
        lock(&self.calls)
            .iter()
            .filter_map(|call| match call {
                AudioCall::Play { wav, volume } => Some((wav.clone(), *volume)),
                AudioCall::Stop | AudioCall::SetVolume(_) => None,
            })
            .collect()
    }

    /// The next play fails with `err`, once.
    pub fn fail_next(&self, err: AudioError) {
        *lock(&self.fail) = Some(err);
    }

    /// Ends the sound now playing, as reaching its end would.
    pub fn finish(&self) {
        lock(&self.current).take();
    }
}

impl AudioOut for RecordingAudio {
    fn play(&self, wav: Vec<u8>, volume: f32) -> Playback {
        lock(&self.calls).push(AudioCall::Play { wav, volume });
        let (tx, playback) = Playback::channel();
        let mut current = lock(&self.current);
        // Replacing the sender drops the previous one, which ends the previous playback.
        *current = match lock(&self.fail).take() {
            Some(err) => {
                let _ = tx.try_send(err);
                None
            }
            None => Some(tx),
        };
        playback
    }

    fn stop(&self) {
        lock(&self.calls).push(AudioCall::Stop);
        lock(&self.current).take();
    }

    fn set_volume(&self, volume: f32) {
        lock(&self.calls).push(AudioCall::SetVolume(volume));
    }
}
