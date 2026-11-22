//! The wgpu renderer: its own device on a worker thread, drawing offscreen into a BGRA target
//! and reading each frame back into a `RenderImage`.

use std::sync::Arc;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gpui_kit::RenderImage;
use tf2tools_core::MapMesh;
use wgpu::util::DeviceExt as _;

use super::math::{padded_row_bytes, unpad_rows};
use super::{DrawRequest, FrameStats, RenderEvent, RendererError, RendererFactory, SceneRenderer};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// `[position; 3, normal; 3]` as f32.
const VERTEX_STRIDE: u64 = 24;
/// The view-projection, then the light direction as a vec4.
const UNIFORM_BYTES: u64 = 80;
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.08,
    g: 0.09,
    b: 0.11,
    a: 1.0,
};

const SHADER: &str = r"
struct Uniforms { view_proj: mat4x4<f32>, light: vec4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Out { @builtin(position) pos: vec4<f32>, @location(0) normal: vec3<f32> };

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>) -> Out {
    var out: Out;
    out.pos = u.view_proj * vec4<f32>(pos, 1.0);
    out.normal = normal;
    return out;
}

// Two-sided: a face lit from behind shades like its front.
@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let shade = 0.35 + 0.65 * abs(dot(normalize(in.normal), u.light.xyz));
    return vec4<f32>(vec3<f32>(0.55 * shade), 1.0);
}
";

// Every worker spawned and not yet joined, so app quit can wait for their device teardown.
static WORKERS: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

enum Command {
    LoadScene(Arc<MapMesh>),
    Draw(DrawRequest),
}

/// The real renderer. Dropping it ends the worker without waiting for it.
pub struct Renderer {
    commands: mpsc::Sender<Command>,
    events: async_channel::Receiver<RenderEvent>,
}

impl Renderer {
    /// Starts the worker; its first event is `Ready` or `Failed`.
    pub fn spawn() -> Self {
        let (commands, inbox) = mpsc::channel();
        let (sender, events) = async_channel::unbounded();
        let worker_sender = sender.clone();
        let spawned = std::thread::Builder::new()
            .name("viewer-render".into())
            .spawn(move || {
                // A panic (wgpu reports validation errors that way) ends the worker as a failure.
                if crate::crash::catching(|| run(&inbox, &worker_sender)).is_err() {
                    let error = RendererError::Device("the render worker panicked".into());
                    let _ = worker_sender.try_send(RenderEvent::Failed(error));
                }
                // The sender drops last, after `run` destroyed the device: closed means torn down.
                drop(worker_sender);
            });
        match spawned {
            Ok(handle) => WORKERS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(handle),
            Err(e) => {
                let error = RendererError::Device(e.to_string());
                let _ = sender.try_send(RenderEvent::Failed(error));
            }
        }
        Self { commands, events }
    }
}

impl SceneRenderer for Renderer {
    fn load_scene(&self, mesh: Arc<MapMesh>) {
        let _ = self.commands.send(Command::LoadScene(mesh));
    }

    fn draw(&self, request: DrawRequest) {
        let _ = self.commands.send(Command::Draw(request));
    }

    fn events(&self) -> async_channel::Receiver<RenderEvent> {
        self.events.clone()
    }
}

/// Joins every finished render worker, waiting up to `timeout` for the rest; true when none
/// is left. For app quit, so no worker is still destroying its device while the process exits.
pub fn join_render_workers(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let finished: Vec<_> = {
            let mut workers = WORKERS.lock().unwrap_or_else(PoisonError::into_inner);
            let (done, running) = workers.drain(..).partition(JoinHandle::is_finished);
            *workers = running;
            done
        };
        for handle in finished {
            let _ = handle.join();
        }
        let left = WORKERS.lock().unwrap_or_else(PoisonError::into_inner).len();
        if left == 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The factory `Services` holds in the real app.
pub fn wgpu_renderers() -> RendererFactory {
    Arc::new(|| Box::new(Renderer::spawn()) as Box<dyn SceneRenderer>)
}

// The worker loop. It sleeps on the inbox only with no frame in flight; otherwise it takes
// what has arrived, collapses draws to the newest, and finishes the in-flight frame after the
// next one is submitted, so rendering N+1 overlaps reading back N.
fn run(inbox: &mpsc::Receiver<Command>, events: &async_channel::Sender<RenderEvent>) {
    let mut gpu = match Gpu::new() {
        Ok(gpu) => gpu,
        Err(error) => {
            let _ = events.try_send(RenderEvent::Failed(error));
            return;
        }
    };
    let _ = events.try_send(RenderEvent::Ready);
    serve(&mut gpu, inbox, events);
    gpu.shutdown();
}

// Handles commands until the inbox disconnects, the UI drops its receiver, or the GPU fails.
fn serve(
    gpu: &mut Gpu,
    inbox: &mpsc::Receiver<Command>,
    events: &async_channel::Sender<RenderEvent>,
) {
    let mut pending: Option<Pending> = None;
    loop {
        let first = if pending.is_some() {
            match inbox.try_recv() {
                Ok(command) => Some(command),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        } else {
            match inbox.recv() {
                Ok(command) => Some(command),
                Err(_) => return,
            }
        };
        let result = match first {
            None => pending
                .take()
                .map_or(Ok(()), |frame| gpu.finish(frame, events))
                .map(|()| false),
            Some(first) => step(gpu, first, inbox, &mut pending, events),
        };
        match result {
            Ok(true) | Err(None) => return,
            Ok(false) => {}
            Err(Some(error)) => {
                let _ = events.try_send(RenderEvent::Failed(error));
                return;
            }
        }
    }
}

// One batch of commands: scenes load in order, only the newest draw renders. Ok(true) means
// the inbox disconnected; Err(None) means the UI dropped its events receiver.
fn step(
    gpu: &mut Gpu,
    first: Command,
    inbox: &mpsc::Receiver<Command>,
    pending: &mut Option<Pending>,
    events: &async_channel::Sender<RenderEvent>,
) -> Result<bool, Option<RendererError>> {
    let mut draw = None;
    let mut next = Some(first);
    let mut disconnected = false;
    while let Some(command) = next {
        match command {
            Command::LoadScene(mesh) => gpu.load_scene(&mesh),
            Command::Draw(request) => draw = Some(request),
        }
        next = match inbox.try_recv() {
            Ok(command) => Some(command),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                disconnected = true;
                None
            }
        };
    }
    if disconnected {
        return Ok(true);
    }
    if let Some(request) = draw {
        let frame = gpu.render(request, pending, events)?;
        if let Some(previous) = pending.replace(frame) {
            gpu.finish(previous, events)?;
        }
    }
    Ok(false)
}

// A submitted frame whose readback slot is mapping.
struct Pending {
    slot: usize,
    seq: u64,
    width: u32,
    height: u32,
    submission: wgpu::SubmissionIndex,
    mapped: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    render: std::time::Duration,
}

struct Targets {
    width: u32,
    height: u32,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    readback: [wgpu::Buffer; 2],
}

struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: Option<(wgpu::Buffer, u32)>,
    targets: Option<Targets>,
    next_slot: usize,
}

impl Gpu {
    fn new() -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|_| RendererError::NoAdapter)?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|e| RendererError::Device(e.to_string()))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewer uniforms"),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewer"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewer"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewer"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            // No culling: vbsp's triangle winding is not consistent, and both sides draw.
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipeline,
            uniforms,
            bind_group,
            vertices: None,
            targets: None,
            next_slot: 0,
        })
    }

    // Destroys the GPU objects resources-first, device and instance last, so nothing outlives
    // what created it.
    fn shutdown(self) {
        let Self {
            instance,
            adapter,
            device,
            queue,
            pipeline,
            uniforms,
            bind_group,
            vertices,
            targets,
            next_slot: _,
        } = self;
        drop(targets);
        drop(vertices);
        drop(bind_group);
        drop(uniforms);
        drop(pipeline);
        drop(queue);
        drop(device);
        drop(adapter);
        drop(instance);
    }

    // Uploads the mesh interleaved; an empty mesh leaves nothing to draw.
    fn load_scene(&mut self, mesh: &MapMesh) {
        let mut bytes = Vec::with_capacity(mesh.positions().len() * VERTEX_STRIDE as usize);
        for (position, normal) in mesh.positions().iter().zip(mesh.normals()) {
            for value in position.iter().chain(normal) {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
        }
        let count = u32::try_from(mesh.positions().len()).unwrap_or(u32::MAX);
        self.vertices = (count > 0).then(|| {
            let buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("viewer vertices"),
                    contents: &bytes,
                    usage: wgpu::BufferUsages::VERTEX,
                });
            (buffer, count)
        });
    }

    // Rebuilds the targets for a new size, finishing the in-flight frame first since it reads
    // from the old ones. Sizes clamp to what the device can hold.
    fn ensure_targets(
        &mut self,
        width: u32,
        height: u32,
        pending: &mut Option<Pending>,
        events: &async_channel::Sender<RenderEvent>,
    ) -> Result<(), Option<RendererError>> {
        let max = self.device.limits().max_texture_dimension_2d;
        let (width, height) = (width.clamp(1, max), height.clamp(1, max));
        if self
            .targets
            .as_ref()
            .is_some_and(|t| (t.width, t.height) == (width, height))
        {
            return Ok(());
        }
        if let Some(frame) = pending.take() {
            self.finish(frame, events)?;
        }
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = |label, format, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            "viewer colour",
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture(
            "viewer depth",
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let readback = [0, 1].map(|_| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("viewer readback"),
                size: u64::from(padded_row_bytes(width)) * u64::from(height),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        self.targets = Some(Targets {
            width,
            height,
            color_view: color.create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            color,
            readback,
        });
        Ok(())
    }

    // Encodes and submits one frame, copies it into the free readback slot and starts mapping.
    fn render(
        &mut self,
        request: DrawRequest,
        pending: &mut Option<Pending>,
        events: &async_channel::Sender<RenderEvent>,
    ) -> Result<Pending, Option<RendererError>> {
        self.ensure_targets(request.width, request.height, pending, events)?;
        let Some(targets) = self.targets.as_ref() else {
            return Err(Some(RendererError::Device("no render target".into())));
        };
        let started = Instant::now();
        let light = {
            let [x, y, z] = [0.3f32, 0.5, 0.8];
            let len = (x * x + y * y + z * z).sqrt();
            [x / len, y / len, z / len, 0.0]
        };
        let mut uniform = Vec::with_capacity(UNIFORM_BYTES as usize);
        for value in request.view_proj.iter().flatten().chain(&light) {
            uniform.extend_from_slice(&value.to_ne_bytes());
        }
        self.queue.write_buffer(&self.uniforms, 0, &uniform);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some((vertices, count)) = &self.vertices {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }
        let slot = self.next_slot;
        self.next_slot = 1 - slot;
        let buffer = &targets.readback[slot];
        encoder.copy_texture_to_buffer(
            targets.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes(targets.width)),
                    rows_per_image: Some(targets.height),
                },
            },
            wgpu::Extent3d {
                width: targets.width,
                height: targets.height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.queue.submit([encoder.finish()]);
        let render = started.elapsed();
        let (done, mapped) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = done.send(result);
            });
        Ok(Pending {
            slot,
            seq: request.seq,
            width: targets.width,
            height: targets.height,
            submission,
            mapped,
            render,
        })
    }

    // Waits for `frame`'s submission, copies its rows out and sends it as a `Frame`.
    fn finish(
        &self,
        frame: Pending,
        events: &async_channel::Sender<RenderEvent>,
    ) -> Result<(), Option<RendererError>> {
        let device = |e: String| Some(RendererError::Device(e));
        let Some(targets) = self.targets.as_ref() else {
            return Err(device("no render target".into()));
        };
        let started = Instant::now();
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(frame.submission),
                timeout: None,
            })
            .map_err(|e| device(e.to_string()))?;
        frame
            .mapped
            .recv()
            .map_err(|e| device(e.to_string()))?
            .map_err(|e| device(e.to_string()))?;
        let buffer = &targets.readback[frame.slot];
        let rows = {
            let view = buffer
                .slice(..)
                .get_mapped_range()
                .map_err(|e| device(e.to_string()))?;
            unpad_rows(&view, frame.width, frame.height)
        };
        buffer.unmap();
        let readback = started.elapsed();
        let started = Instant::now();
        let pixels = image::RgbaImage::from_raw(frame.width, frame.height, rows)
            .ok_or_else(|| device("short readback".into()))?;
        let image = Arc::new(RenderImage::new([image::Frame::new(pixels)]));
        let stats = FrameStats {
            render: frame.render,
            readback,
            image: started.elapsed(),
        };
        events
            .try_send(RenderEvent::Frame {
                image,
                seq: frame.seq,
                stats,
            })
            .map_err(|_| None)
    }
}
