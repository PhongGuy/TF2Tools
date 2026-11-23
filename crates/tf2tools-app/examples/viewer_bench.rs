//! `viewer_bench <map.bsp> <frames>`: times the viewer's renderer on one map at four sizes.
//!
//! Prints the adapter, then one line per size and stage with p50, p99 and max.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use tf2tools_app::viewer::math::view_proj;
use tf2tools_app::viewer::stats::{Percentiles, percentiles};
use tf2tools_app::viewer::{DrawRequest, RenderEvent, Renderer, SceneRenderer};
use tf2tools_core::{MapMesh, load_map_file};

const SIZES: [(u32, u32); 4] = [(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)];
/// Draws kept in flight, so rendering one frame overlaps reading back the previous.
const IN_FLIGHT: u64 = 2;

fn main() -> ExitCode {
    let code = match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("viewer_bench: {reason}");
            ExitCode::FAILURE
        }
    };
    // Let each worker finish dropping its device first, or it races libc exit and segfaults.
    tf2tools_app::viewer::join_render_workers(Duration::from_secs(10));
    code
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [map, frames] = args.as_slice() else {
        return Err("usage: viewer_bench <map.bsp> <frames>".into());
    };
    let frames: u64 = frames
        .parse()
        .ok()
        .filter(|n| *n > 0)
        .ok_or("frames must be a positive number")?;
    let mesh = Arc::new(load_map_file(Path::new(map)).map_err(|e| format!("{map}: {e}"))?);
    if mesh.triangle_count() == 0 {
        return Err(format!("{map}: the map has no visible faces"));
    }
    println!("adapter {}", adapter_name()?);
    println!(
        "{} triangles, {frames} frames per size",
        mesh.triangle_count()
    );
    for (width, height) in SIZES {
        bench_size(&mesh, width, height, frames)?;
    }
    Ok(())
}

// The adapter the renderer's own selection picks: same instance setup and power preference.
fn adapter_name() -> Result<String, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let options = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    };
    let adapter = pollster::block_on(instance.request_adapter(&options))
        .map_err(|_| "no GPU adapter".to_owned())?;
    let info = adapter.get_info();
    Ok(format!("{} ({:?})", info.name, info.backend))
}

fn bench_size(mesh: &Arc<MapMesh>, width: u32, height: u32, frames: u64) -> Result<(), String> {
    let renderer = Renderer::spawn();
    let events = renderer.events();
    match events.recv_blocking() {
        Ok(RenderEvent::Ready) => {}
        Ok(RenderEvent::Failed(error)) => return Err(error.to_string()),
        Ok(RenderEvent::Frame { .. }) | Err(_) => return Err("the renderer did not start".into()),
    }
    renderer.load_scene(Arc::clone(mesh));
    let draw = |seq: u64| {
        let (eye, yaw) = orbit(mesh, seq, frames);
        renderer.draw(DrawRequest {
            width,
            height,
            view_proj: view_proj(eye, yaw, 20.0, width as f32 / height as f32),
            seq,
        });
    };
    let mut sent = 0;
    while sent < IN_FLIGHT.min(frames) {
        sent += 1;
        draw(sent);
    }
    let (mut render, mut readback, mut image) = (Vec::new(), Vec::new(), Vec::new());
    let mut received = 0;
    // Draws the worker collapses away never come back, so count what arrives, not what was sent.
    while received < frames {
        match events.recv_blocking() {
            Ok(RenderEvent::Frame { stats, seq, .. }) => {
                received += 1;
                render.push(stats.render);
                readback.push(stats.readback);
                image.push(stats.image);
                if sent < frames {
                    sent += 1;
                    draw(sent);
                }
                if seq >= frames {
                    break;
                }
            }
            Ok(RenderEvent::Ready) => {}
            Ok(RenderEvent::Failed(error)) => return Err(error.to_string()),
            Err(_) => return Err("the renderer stopped early".into()),
        }
    }
    for (stage, mut samples) in [("render", render), ("readback", readback), ("image", image)] {
        print_line(width, height, stage, percentiles(&mut samples));
    }
    Ok(())
}

// Camera `seq` of `frames` on one turn around the bounds centre, looking at it.
fn orbit(mesh: &MapMesh, seq: u64, frames: u64) -> ([f32; 3], f32) {
    let (min, max) = mesh.bounds();
    let centre = [0, 1, 2].map(|i| (min[i] + max[i]) / 2.0);
    let diagonal = (0..3)
        .map(|i| (max[i] - min[i]).powi(2))
        .sum::<f32>()
        .sqrt();
    let radius = diagonal / 2.0;
    let angle = std::f32::consts::TAU * seq as f32 / frames as f32;
    let eye = [
        centre[0] + radius * angle.cos(),
        centre[1] + radius * angle.sin(),
        centre[2] + 256.0,
    ];
    let yaw = (centre[1] - eye[1]).atan2(centre[0] - eye[0]).to_degrees();
    (eye, yaw)
}

fn print_line(width: u32, height: u32, stage: &str, p: Option<Percentiles>) {
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    match p {
        Some(p) => println!(
            "{width}x{height} {stage} p50 {:.2} ms p99 {:.2} ms max {:.2} ms",
            ms(p.p50),
            ms(p.p99),
            ms(p.max)
        ),
        None => println!("{width}x{height} {stage} no frames"),
    }
}
