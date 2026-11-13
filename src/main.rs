mod capture;
mod pipeline;
mod segmenter;
mod sink;
mod stats;
mod vcam;

use nokhwa::pixel_format::RgbFormat;
use nokhwa::Camera;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use pipeline::Pipeline;
use segmenter::Segmenter;
use sink::{Sink, WindowSink};

const USAGE: &str = "ZagCam - lightweight virtual camera

Usage:
  zagcam [options]

Options:
  --camera N        camera number to open (see --list, default 0)
  --list, -l        list available cameras
  --mode off|blur   effect (default off)
  --blur 0..100     blur intensity (default 50)
  --sink window|vcam  output: preview window or virtual camera (default window)
  --model PATH      ONNX segmentation model
  --probe S         run S seconds headless, print fps / RAM / CPU
  --help, -h        show this help

Commands:
  vcam <cmd>        manage the virtual camera (start, stop, status, install, uninstall)

Keys: Esc quits the preview window.";

const DEFAULT_MODEL: &str = "models/selfie_segmentation_landscape.onnx";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Off,
    Blur,
}

#[derive(Clone, Copy, PartialEq)]
enum SinkKind {
    Window,
    Vcam,
}

struct Args {
    list: bool,
    camera: Option<usize>,
    mode: Mode,
    blur: u8,
    sink: SinkKind,
    model: PathBuf,
    probe: Option<f64>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("zagcam: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("vcam") {
        argv.remove(0);
        return vcam::run(&argv);
    }
    let args = parse_args(argv)?;
    if args.list {
        let cameras = capture::list()?;
        for (i, cam) in cameras.iter().enumerate() {
            let name = cam.human_name();
            let kind = if name.to_ascii_lowercase().contains("virtual camera") { " (virtual)" } else { "" };
            println!("{i}: {name} [{}]{kind}", cam.index().as_index().unwrap_or(0));
        }
        return Ok(());
    }

    let cameras = capture::list()?;
    if cameras.is_empty() {
        return Err("no camera found".into());
    }
    let source = match args.camera {
        Some(i) => cameras.get(i).ok_or_else(|| format!("camera {i} not found"))?,
        None => &cameras[0],
    };

    let mut camera = capture::open(source.index())?;
    let format = camera.camera_format();
    let (sw, sh) = (format.width() as usize, format.height() as usize);
    println!("source: {} | {}x{} {}", source.human_name(), sw, sh, format.format());

    let mut rgb = vec![0u8; sw * sh * 3];

    let mut pipeline = match args.mode {
        Mode::Off => None,
        Mode::Blur => {
            let segmenter = Segmenter::load(&args.model)?;
            Some(Pipeline::new(segmenter, sw, sh, args.blur)?)
        }
    };

    if let Some(secs) = args.probe {
        return probe(&mut camera, &mut rgb, secs, pipeline.as_mut());
    }

    let mut output = make_sink(args.sink, sw, sh)?;
    run_loop(&mut camera, &mut rgb, pipeline.as_mut(), output.as_mut())
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut args = Args {
        list: false,
        camera: None,
        mode: Mode::Off,
        blur: 50,
        sink: SinkKind::Window,
        model: PathBuf::from(DEFAULT_MODEL),
        probe: None,
    };
    let mut raw = argv.into_iter();
    while let Some(arg) = raw.next() {
        let mut value = || raw.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--list" | "-l" => args.list = true,
            "--camera" | "-c" => {
                args.camera = Some(value()?.parse().map_err(|_| "bad camera index".to_string())?);
            }
            "--mode" => {
                args.mode = match value()?.as_str() {
                    "off" => Mode::Off,
                    "blur" => Mode::Blur,
                    other => return Err(format!("unknown mode {other}")),
                };
            }
            "--blur" => {
                args.blur = value()?.parse().map_err(|_| "bad blur intensity".to_string())?;
            }
            "--sink" => {
                args.sink = match value()?.as_str() {
                    "window" => SinkKind::Window,
                    "vcam" => SinkKind::Vcam,
                    other => return Err(format!("unknown sink {other}")),
                };
            }
            "--model" => args.model = PathBuf::from(value()?),
            "--probe" => {
                args.probe = Some(value()?.parse().map_err(|_| "bad probe duration".to_string())?);
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }
    Ok(args)
}

fn make_sink(kind: SinkKind, w: usize, h: usize) -> Result<Box<dyn Sink>, String> {
    match kind {
        SinkKind::Window => Ok(Box::new(WindowSink::new(w, h)?)),
        SinkKind::Vcam => {
            #[cfg(feature = "sink-vcam")]
            {
                Ok(Box::new(sink::VcamSink::new(w as u32, h as u32, 30.0)?))
            }
            #[cfg(not(feature = "sink-vcam"))]
            {
                let _ = (w, h);
                Err("this build has no virtual-camera output; rebuild with --features sink-vcam".into())
            }
        }
    }
}

fn run_loop(
    camera: &mut Camera,
    rgb: &mut [u8],
    mut pipeline: Option<&mut Pipeline>,
    output: &mut dyn Sink,
) -> Result<(), String> {
    let mut sampler = stats::Sampler::new();
    let start = Instant::now();
    let mut last_stats = Instant::now();
    let mut frames: u64 = 0;
    let size = camera.camera_format();
    let (w, h) = (size.width() as usize, size.height() as usize);

    while output.alive() {
        next_frame(camera, rgb)?;
        let present: &[u8] = match pipeline.as_deref_mut() {
            Some(pipeline) => pipeline.process(rgb)?,
            None => rgb,
        };
        output.present(present, w, h)?;
        frames += 1;

        if last_stats.elapsed() >= Duration::from_secs(1) {
            let snap = sampler.snapshot();
            output.set_title(&snap.title(frames, start.elapsed()));
            last_stats = Instant::now();
        }
    }

    let snap = sampler.snapshot();
    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "M1 session: {:.1} fps over {:.1}s | working set {:.1} MiB | CPU {:.1}% of one core",
        frames as f64 / elapsed,
        elapsed,
        snap.working_set as f64 / 1_048_576.0,
        snap.cpu_percent
    );
    Ok(())
}

fn probe(
    camera: &mut Camera,
    rgb: &mut [u8],
    secs: f64,
    mut pipeline: Option<&mut Pipeline>,
) -> Result<(), String> {
    let mut sampler = stats::Sampler::new();
    let start = Instant::now();
    let mut frames: u64 = 0;
    while start.elapsed().as_secs_f64() < secs {
        next_frame(camera, rgb)?;
        if let Some(pipeline) = pipeline.as_deref_mut() {
            pipeline.process(rgb)?;
        }
        frames += 1;
    }
    let elapsed = start.elapsed().as_secs_f64();
    let snap = sampler.snapshot();
    println!(
        "M1 baseline: {:.1} fps over {:.1}s | working set {:.1} MiB | CPU {:.1}% of one core",
        frames as f64 / elapsed,
        elapsed,
        snap.working_set as f64 / 1_048_576.0,
        snap.cpu_percent
    );
    if let Some(pipeline) = pipeline.as_deref()
        && pipeline.samples > 0
    {
        let n = pipeline.samples as f64;
        let t = pipeline.timings;
        println!(
            "  per frame: preprocess {:.2} ms | infer/every-2 {:.2} ms | mask {:.2} ms | blur {:.2} ms | composite {:.2} ms",
            t.preprocess.as_secs_f64() * 1000.0 / n,
            t.infer.as_secs_f64() * 1000.0 / n,
            t.mask.as_secs_f64() * 1000.0 / n,
            t.blur.as_secs_f64() * 1000.0 / n,
            t.composite.as_secs_f64() * 1000.0 / n
        );
    }
    Ok(())
}

fn next_frame(camera: &mut Camera, rgb: &mut [u8]) -> Result<(), String> {
    let frame = camera.frame().map_err(|why| format!("capture failed: {why}"))?;
    frame
        .decode_image_to_buffer::<RgbFormat>(rgb)
        .map_err(|why| format!("decode failed: {why}"))
}
