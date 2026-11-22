//! The demo viewer's 3D view: camera math and a renderer worker that draws a map offscreen and
//! hands each frame back as a GPUI image.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::RenderImage;
use tf2tools_core::MapMesh;

pub mod load;
pub mod math;
pub mod renderer;
pub mod viewport;

pub use renderer::{Renderer, join_render_workers, wgpu_renderers};

/// One frame to draw: its pixel size, the camera, and a sequence number the frame echoes back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawRequest {
    pub width: u32,
    pub height: u32,
    pub view_proj: [[f32; 4]; 4],
    pub seq: u64,
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
