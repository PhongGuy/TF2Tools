//! The real speakers: one audio thread owns rodio's device and the playing sound.

use std::any::Any;
use std::io::Cursor;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use rodio::Decoder;
use rodio::mixer::Mixer;

use super::{AudioError, AudioOut, Playback};

/// How often the audio thread checks whether the playing sound has ended.
const POLL: Duration = Duration::from_millis(25);

/// Where sounds play: a mixer, and whatever must stay alive for it to be heard.
pub struct Output {
    mixer: Mixer,
    _keep_alive: Box<dyn Any>,
}

impl Output {
    /// Plays into `mixer`; `keep_alive` (the device sink, say) is dropped with the output.
    pub fn new(mixer: Mixer, keep_alive: impl Any) -> Self {
        Self {
            mixer,
            _keep_alive: Box::new(keep_alive),
        }
    }
}

enum Command {
    Play {
        wav: Vec<u8>,
        volume: f32,
        ended: async_channel::Sender<AudioError>,
    },
    Stop,
    SetVolume(f32),
}

/// `AudioOut` over rodio. Calls queue commands for the audio thread and never block.
pub struct RodioOut {
    commands: Sender<Command>,
}

impl RodioOut {
    /// The default output device, opened on the first play.
    pub fn new() -> Self {
        Self::with_output(open_default)
    }

    /// Plays into what `open` returns; `open` runs on the audio thread at the first play
    /// and again after a failed open.
    pub fn with_output(open: impl FnMut() -> Result<Output, AudioError> + Send + 'static) -> Self {
        let (commands, inbox) = mpsc::channel();
        // A failed spawn leaves the receiver dropped, so every play reports `Closed`.
        let _ = std::thread::Builder::new()
            .name("tf2tools-audio".into())
            .spawn(move || run(inbox, open));
        Self { commands }
    }
}

impl Default for RodioOut {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioOut for RodioOut {
    fn play(&self, wav: Vec<u8>, volume: f32) -> Playback {
        let (ended, playback) = Playback::channel();
        if let Err(mpsc::SendError(Command::Play { ended, .. })) =
            self.commands.send(Command::Play { wav, volume, ended })
        {
            let _ = ended.try_send(AudioError::Closed);
        }
        playback
    }

    fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    fn set_volume(&self, volume: f32) {
        let _ = self.commands.send(Command::SetVolume(volume));
    }
}

fn open_default() -> Result<Output, AudioError> {
    let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    // Otherwise rodio prints a notice to stderr when the sink drops.
    sink.log_on_drop(false);
    Ok(Output::new(sink.mixer().clone(), sink))
}

// One sound at a time. The device opens on the first play and stays open; a failed open is
// retried by the next play. While a sound plays the loop polls, so a finished sound ends its
// playback; with nothing playing it sleeps until the next command.
fn run(inbox: Receiver<Command>, mut open: impl FnMut() -> Result<Output, AudioError>) {
    let mut output: Option<Output> = None;
    // Dropping the pair stops the sound and closes its playback.
    let mut playing: Option<(rodio::Player, async_channel::Sender<AudioError>)> = None;
    loop {
        let command = match playing {
            Some(_) => inbox.recv_timeout(POLL),
            None => inbox.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match command {
            Ok(Command::Play { wav, volume, ended }) => {
                playing = None;
                match start(&mut output, &mut open, wav, volume) {
                    Ok(player) => playing = Some((player, ended)),
                    Err(err) => {
                        let _ = ended.try_send(err);
                    }
                }
            }
            Ok(Command::Stop) => playing = None,
            Ok(Command::SetVolume(volume)) => {
                if let Some((player, _)) = &playing {
                    player.set_volume(volume);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if playing.as_ref().is_some_and(|(player, _)| player.empty()) {
            playing = None;
        }
    }
}

// Decodes first, so a bad file never opens the device.
fn start(
    output: &mut Option<Output>,
    open: &mut impl FnMut() -> Result<Output, AudioError>,
    wav: Vec<u8>,
    volume: f32,
) -> Result<rodio::Player, AudioError> {
    let source = Decoder::new(Cursor::new(wav)).map_err(|e| AudioError::Decode(e.to_string()))?;
    let output = match output {
        Some(output) => output,
        None => output.insert(open()?),
    };
    let player = rodio::Player::connect_new(&output.mixer);
    player.set_volume(volume);
    player.append(source);
    Ok(player)
}
