//! The demo viewer's 3D view: camera math and a renderer worker that draws a map offscreen and
//! hands each frame back as a GPUI image.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use gpui_kit::RenderImage;
use tf2tools_core::{MapMesh, PlaybackEvent, Team};

pub mod load;
pub mod math;
pub mod pointer;
pub mod renderer;
pub mod stats;
pub mod viewport;

pub use renderer::{Renderer, join_render_workers, wgpu_renderers};

/// One frame to draw: its pixel size, the camera, a sequence number the frame echoes back,
/// and the player boxes to draw in it.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawRequest {
    pub width: u32,
    pub height: u32,
    pub view_proj: [[f32; 4]; 4],
    pub seq: u64,
    pub players: Arc<[PlayerBox]>,
}

/// Where one frame's time went on the worker.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameStats {
    /// Encoding the frame up to its submit.
    pub render: Duration,
    /// Waiting for the readback mapping, plus copying the rows out.
    pub readback: Duration,
    /// Building the `RenderImage`.
    pub image: Duration,
}

/// What a renderer reports back to the UI.
pub enum RenderEvent {
    /// The device is up; draws will produce frames.
    Ready,
    /// A finished frame, BGRA, for the request with `seq`.
    Frame {
        image: Arc<RenderImage>,
        seq: u64,
        stats: FrameStats,
    },
    /// The renderer stopped and will send nothing more.
    Failed(RendererError),
}

/// Why a renderer stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RendererError {
    #[error("no GPU adapter")]
    NoAdapter,
    #[error("GPU device failed: {0}")]
    Device(String),
}

/// One player drawn as a box: where its feet stand, which way it faces, and its team.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerBox {
    pub feet: [f32; 3],
    pub yaw_deg: f32,
    pub ducked: bool,
    pub team: Team,
}

/// A map renderer the viewport drives; the real one runs wgpu on its own thread.
pub trait SceneRenderer {
    /// Replaces the scene drawn by later draws.
    fn load_scene(&self, mesh: Arc<MapMesh>);
    /// Asks for a frame. Draws not yet started when a newer one arrives are dropped.
    fn draw(&self, request: DrawRequest);
    /// The renderer's events; closed once the renderer has stopped.
    fn events(&self) -> async_channel::Receiver<RenderEvent>;
}

/// Makes a renderer for a page; tests swap the wgpu one for a stub.
pub type RendererFactory = Arc<dyn Fn() -> Box<dyn SceneRenderer> + Send + Sync>;

/// Reads a demo's still picture and playback tracks, sending each event as it becomes known
/// until the cancel flag is set or the sink answers `Break`; tests swap in scripted events.
pub type PlaybackReader =
    Arc<dyn Fn(&Path, &AtomicBool, &mut dyn FnMut(PlaybackEvent) -> ControlFlow<()>) + Send + Sync>;

/// The reader `Services` holds in the real app: `tf2tools_core::read_demo_playback`.
pub fn core_playback() -> PlaybackReader {
    Arc::new(|dem, cancel, emit| tf2tools_core::read_demo_playback(dem, cancel, emit))
}
