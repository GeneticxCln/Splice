use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use splice::app::{AppOptions, SpliceApp};
use splice::export::export_project;
use splice::ffmpeg::Toolchain;
use splice::presets::ExportPreset;
use splice::project_io::load_project;

#[derive(Debug, Parser)]
#[command(
    name = "splice",
    version,
    about = "Splice — native Linux timeline video editor",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    /// Project to open in the editor.
    project: Option<PathBuf>,
    /// Capture the real GUI window to a PNG, then keep running unless
    /// --exit-after-screenshot is also passed.
    #[arg(long, value_name = "file.png")]
    screenshot: Option<PathBuf>,
    /// Exit once the requested screenshot has been written.
    #[arg(long, requires = "screenshot")]
    exit_after_screenshot: bool,
    /// Initial playhead, in seconds (useful for verification screenshots).
    #[arg(long, default_value_t = 0.0, value_name = "seconds")]
    playhead: f64,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Export a project to H.264/AAC MP4 without opening the GUI.
    Export {
        /// Path to a .splice project.
        #[arg(long, value_name = "file.splice")]
        project: PathBuf,
        /// Locked export preset.
        #[arg(long, value_enum)]
        preset: ExportPreset,
        /// Destination .mp4 file.
        #[arg(long, value_name = "file.mp4")]
        output: PathBuf,
    },
    /// Print ffprobe metadata for a media file.
    Probe {
        #[arg(value_name = "file")]
        file: PathBuf,
    },
    /// Record from the default input through cpal and verify it is non-silent.
    Voicecheck {
        #[arg(long, default_value_t = 4.0)]
        seconds: f64,
        #[arg(long, value_name = "recording.wav")]
        output: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Commands::Export {
            project,
            preset,
            output,
        }) => run_export(&project, preset, &output),
        Some(Commands::Probe { file }) => run_probe(&file),
        Some(Commands::Voicecheck { seconds, output }) => run_voicecheck(seconds, &output),
        None => run_gui(
            cli.project,
            cli.screenshot,
            cli.exit_after_screenshot,
            cli.playhead,
        ),
    };

    if let Err(error) = result {
        eprintln!("splice: {error:#}");
        std::process::exit(1);
    }
}

fn run_gui(
    project: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    exit_after_screenshot: bool,
    playhead: f64,
) -> Result<()> {
    let options = AppOptions {
        project,
        screenshot,
        exit_after_screenshot,
        playhead,
    };
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Splice")
            .with_app_id("splice")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1180.0, 720.0]),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "Splice",
        native_options,
        Box::new(move |cc| {
            let app = SpliceApp::new(cc, options).map_err(|error| {
                Box::new(std::io::Error::other(format!("{error:#}")))
                    as Box<dyn std::error::Error + Send + Sync>
            })?;
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| anyhow::anyhow!("could not start the GUI: {error}"))?;
    Ok(())
}

fn run_export(project_path: &Path, preset: ExportPreset, output: &Path) -> Result<()> {
    let tools = Toolchain::detect().context("system ffmpeg/ffprobe is required")?;
    tools.require_encoders()?;
    let loaded = load_project(project_path)?;
    let project_dir = loaded
        .path
        .parent()
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let missing = loaded.project.missing_assets(&project_dir);
    if !missing.is_empty() {
        let details = missing
            .iter()
            .map(|asset| {
                format!(
                    "  {} => {}",
                    asset.name,
                    splice::model::resolve_media_path(&project_dir, asset).display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!(
            "export blocked: {} missing media asset(s) in {}\n{}",
            missing.len(),
            project_path.display(),
            details
        );
    }

    let report = export_project(&loaded.project, &project_dir, &tools, preset, output)?;
    println!(
        "EXPORT_OK {} preset={} duration={:.2}s",
        report.output.display(),
        report.preset.label(),
        report.duration
    );
    Ok(())
}

fn run_probe(file: &Path) -> Result<()> {
    let tools = Toolchain::detect().context("system ffmpeg/ffprobe is required")?;
    let info = tools.probe(file)?;
    println!(
        "PROBE_OK {} kind={:?} duration={:.3}s size={}x{} audio={} rate={} ch={} format={}",
        file.display(),
        info.kind,
        info.duration,
        info.width.unwrap_or(0),
        info.height.unwrap_or(0),
        info.has_audio,
        info.sample_rate.unwrap_or(0),
        info.channels.unwrap_or(0),
        info.format_name
    );
    Ok(())
}

fn run_voicecheck(seconds: f64, output: &Path) -> Result<()> {
    let input = splice::audio::describe_default_input()?;
    println!("DEFAULT_INPUT {input}");
    let stats = splice::audio::record_for(seconds, output)?;
    println!(
        "VOICECHECK device=\"{}\" host={} sample_rate={} channels={} seconds={:.3} rms_dbfs={:.2} peak_dbfs={:.2} non_silent={} path={}",
        stats.device,
        stats.host,
        stats.sample_rate,
        stats.channels,
        stats.seconds,
        stats.rms_dbfs,
        stats.peak_dbfs,
        stats.non_silent(),
        stats.path.display()
    );
    if !stats.non_silent() {
        anyhow::bail!(
            "recording is silent (RMS {:.1} dBFS, peak {:.1} dBFS); check the default microphone",
            stats.rms_dbfs,
            stats.peak_dbfs
        );
    }
    Ok(())
}
