//! Entry point for Svarog3D.
//! A2: logging + CLI backend flag.

use anyhow::Result;
use wgpu;

fn parse_backend_arg() -> wgpu::Backends {
    // Accept: --gpu-backend=auto|vulkan|dx12|metal|gl
    let mut backends = wgpu::Backends::all(); // default = auto
    for arg in std::env::args() {
        if let Some(val) = arg.strip_prefix("--gpu-backend=") {
            backends = match val.to_ascii_lowercase().as_str() {
                "auto" => wgpu::Backends::all(),
                "vulkan" | "vk" => wgpu::Backends::VULKAN,
                "dx12" | "d3d12" => wgpu::Backends::DX12,
                "metal" | "mtl" => wgpu::Backends::METAL,
                "gl" | "opengl" | "gles" => wgpu::Backends::GL,
                other => {
                    eprintln!("[warn] Unknown backend '{}', falling back to auto.", other);
                    wgpu::Backends::all()
                }
            };
        }
    }
    backends
}

fn parse_show_fps_arg() -> bool {
    // --show-fps[=on|off], по умолчанию off
    for arg in std::env::args() {
        if arg == "--show-fps" {
            return true;
        }
        if let Some(val) = arg.strip_prefix("--show-fps=") {
            return matches!(
                val.to_ascii_lowercase().as_str(),
                "1" | "true" | "on" | "yes"
            );
        }
    }
    false
}

fn parse_size_args() -> (u32, u32) {
    let mut w: Option<u32> = None;
    let mut h: Option<u32> = None;

    for arg in std::env::args() {
        if let Some(v) = arg.strip_prefix("--size=") {
            if let Some((sw, sh)) = v.split_once('x').or_else(|| v.split_once('X')) {
                if let (Ok(pw), Ok(ph)) = (sw.parse::<u32>(), sh.parse::<u32>()) {
                    w = Some(pw);
                    h = Some(ph);
                }
            }
        } else if let Some(v) = arg.strip_prefix("--width=") {
            if let Ok(pw) = v.parse::<u32>() {
                w = Some(pw);
            }
        } else if let Some(v) = arg.strip_prefix("--height=") {
            if let Ok(ph) = v.parse::<u32>() {
                h = Some(ph);
            }
        }
    }

    let ww = w.unwrap_or(1280).max(1);
    let hh = h.unwrap_or(720).max(1);
    (ww, hh)
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let chosen = parse_backend_arg();
    let show_fps = parse_show_fps_arg();
    let (width, height) = parse_size_args();
    log::info!(
        "Starting Svarog3D (A2/B3). Backend: {:?}, show_fps={}, window_size={}x{}",
        chosen,
        show_fps,
        width,
        height
    );

    platform::run_with_renderer(chosen, show_fps, width, height)?;

    log::info!("Graceful shutdown. Bye!");
    Ok(())
}
