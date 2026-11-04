//! The app's speakers behind a swappable, non-blocking trait.

pub mod player;
mod recording;
mod rodio_out;
pub mod volume;

pub use recording::{AudioCall, RecordingAudio};
pub use rodio_out::{Output, RodioOut};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AudioError {
    #[error("Could not open the audio device: {0}")]
    Device(String),
    #[error("Could not decode the sound: {0}")]
    Decode(String),
    #[error("The audio thread has stopped")]
    Closed,
}

/// How one play ends: an error if the sound never started, else the end of the sound
/// (played out, stopped, or replaced by the next play).
#[derive(Debug)]
pub struct Playback(async_channel::Receiver<AudioError>);

impl Playback {
    /// For `AudioOut` implementations: send at most one error if the sound fails to start;
    /// dropping the sender ends the playback.
    pub fn channel() -> (async_channel::Sender<AudioError>, Self) {
        let (tx, rx) = async_channel::bounded(1);
        (tx, Self(rx))
    }

    /// Waits for the end: a sent error is `Err`, a closed channel is `Ok`.
    pub async fn ended(self) -> Result<(), AudioError> {
        match self.0.recv().await {
            Ok(err) => Err(err),
            Err(_) => Ok(()),
        }
    }

    /// The outcome if the playback already ended, `None` while it plays. It consumes a sent
    /// error, so act on the first `Some`: a later poll after a failed start says `Ok(())`.
    pub fn try_ended(&self) -> Option<Result<(), AudioError>> {
        match self.0.try_recv() {
            Ok(err) => Some(Err(err)),
            Err(async_channel::TryRecvError::Closed) => Some(Ok(())),
            Err(async_channel::TryRecvError::Empty) => None,
        }
    }
}

/// The app's speakers. Volume is linear, 0.0 to 1.0. No call blocks.
pub trait AudioOut: Send + Sync {
    /// Replaces whatever plays with `wav` at `volume`.
    fn play(&self, wav: Vec<u8>, volume: f32) -> Playback;
    fn stop(&self);
    /// Sets the playing sound's volume; the next play brings its own.
    fn set_volume(&self, volume: f32);
}
