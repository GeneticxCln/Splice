use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::model::MediaKind;

#[derive(Debug, Clone)]
pub struct Toolchain {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    pub libx264: bool,
    pub aac: bool,
    pub ass_filter: bool,
    pub fontconfig: bool,
}

#[derive(Debug, Clone)]
pub struct ProbeInfo {
    pub kind: MediaKind,
    pub duration: f64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub has_audio: bool,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub format_name: String,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
}

impl Toolchain {
    pub fn detect() -> Result<Self> {
        let ffmpeg = which("ffmpeg").context("system ffmpeg was not found on PATH")?;
        let ffprobe = which("ffprobe").context("system ffprobe was not found on PATH")?;

        let ffmpeg_version = first_line(command_stdout(&ffmpeg, &["-version"])?);
        let ffprobe_version = first_line(command_stdout(&ffprobe, &["-version"])?);
        let encoders = command_stdout(&ffmpeg, &["-hide_banner", "-encoders"])?;
        let ass_filter = Command::new(&ffmpeg)
            .args(["-hide_banner", "-h", "filter=ass"])
            .output()
            .map(|output| {
                output.status.success()
                    && (String::from_utf8_lossy(&output.stdout).contains("Render ASS subtitles")
                        || String::from_utf8_lossy(&output.stderr).contains("Render ASS subtitles"))
            })
            .unwrap_or(false);
        let fontconfig = which("fc-match").is_ok();

        Ok(Self {
            ffmpeg,
            ffprobe,
            ffmpeg_version,
            ffprobe_version,
            libx264: encoders.contains("libx264"),
            aac: encoders.contains(" aac "),
            ass_filter,
            fontconfig,
        })
    }

    pub fn require_encoders(&self) -> Result<()> {
        if !self.libx264 {
            bail!("system ffmpeg does not provide the libx264 encoder");
        }
        if !self.aac {
            bail!("system ffmpeg does not provide the AAC encoder");
        }
        Ok(())
    }

    /// Capabilities needed for the full feature set. Missing entries are
    /// surfaced in the status bar and the About window instead of being
    /// discovered only when an export fails.
    pub fn capability_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if !self.libx264 {
            warnings.push("libx264 encoder missing".to_string());
        }
        if !self.aac {
            warnings.push("AAC encoder missing".to_string());
        }
        if !self.ass_filter {
            warnings.push("ffmpeg 'ass' filter missing (titles unavailable)".to_string());
        }
        if !self.fontconfig {
            warnings.push("fc-match missing (title font fallback unverified)".to_string());
        }
        warnings
    }

    pub fn probe(&self, path: &Path) -> Result<ProbeInfo> {
        if !path.is_file() {
            bail!("media file does not exist: {}", path.display());
        }
        let output = Command::new(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
            ])
            .arg(path)
            .output()
            .with_context(|| format!("failed to run ffprobe on {}", path.display()))?;

        if !output.status.success() {
            bail!(
                "ffprobe rejected {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let json: Value = serde_json::from_slice(&output.stdout)
            .with_context(|| format!("ffprobe returned invalid JSON for {}", path.display()))?;
        parse_probe(&json, path)
    }

    pub fn export_encoder(&self) -> &Path {
        &self.ffmpeg
    }

    pub fn version_summary(&self) -> String {
        format!(
            "{}\n{}\nlibx264: {}   AAC: {}   ASS filter: {}   fontconfig: {}",
            self.ffmpeg_version,
            self.ffprobe_version,
            if self.libx264 { "yes" } else { "NO" },
            if self.aac { "yes" } else { "NO" },
            if self.ass_filter { "yes" } else { "NO" },
            if self.fontconfig { "yes" } else { "NO" }
        )
    }
}

fn which(binary: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(binary);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("{binary} not found")
}

fn command_stdout(binary: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", binary.display()))?;
    if !output.status.success() {
        bail!(
            "{} {:?} failed: {}",
            binary.display(),
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn first_line(text: String) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

fn parse_probe(json: &Value, path: &Path) -> Result<ProbeInfo> {
    let streams = json
        .get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let format = json.get("format").cloned().unwrap_or(Value::Null);

    let video = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"));
    let audio = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"));

    if video.is_none() && audio.is_none() {
        bail!("{} contains no audio or video stream", path.display());
    }

    let format_name = format
        .get("format_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let is_image = video.is_some()
        && audio.is_none()
        && (looks_like_image(path)
            || format_name.contains("image2")
            || format_name.contains("png_pipe")
            || format_name.contains("jpeg_pipe"));

    let kind = if is_image {
        MediaKind::Image
    } else if video.is_some() {
        MediaKind::Video
    } else {
        MediaKind::Audio
    };

    let duration = format
        .get("duration")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| {
            video
                .and_then(|stream| stream.get("duration"))
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<f64>().ok())
        })
        .or_else(|| {
            audio
                .and_then(|stream| stream.get("duration"))
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<f64>().ok())
        })
        .unwrap_or(0.0);

    let width = video
        .and_then(|stream| stream.get("width"))
        .and_then(Value::as_u64)
        .map(|value| value as u32);
    let height = video
        .and_then(|stream| stream.get("height"))
        .and_then(Value::as_u64)
        .map(|value| value as u32);
    let sample_rate = audio
        .and_then(|stream| stream.get("sample_rate"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<u32>().ok());
    let channels = audio
        .and_then(|stream| stream.get("channels"))
        .and_then(Value::as_u64)
        .map(|value| value as u32);

    Ok(ProbeInfo {
        kind,
        duration,
        width,
        height,
        has_audio: audio.is_some(),
        sample_rate,
        channels,
        format_name,
        video_codec: video
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        audio_codec: audio
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn looks_like_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "bmp" | "avif" | "tif" | "tiff")
    )
}
