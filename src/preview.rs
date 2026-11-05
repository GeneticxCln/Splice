use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, Receiver};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use crate::audio::AudioMonitor;
use crate::ffmpeg::Toolchain;
use crate::model::Project;
use crate::render::{render_project, RenderOptions};

pub const PREVIEW_WIDTH: u32 = 960;
pub const PREVIEW_HEIGHT: u32 = 540;

enum RenderMessage {
    Ready {
        generation: u64,
        path: PathBuf,
        duration: f64,
    },
    Failed {
        generation: u64,
        error: String,
    },
}

struct FrameMessage {
    generation: u64,
    time: f64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    error: Option<String>,
}

struct PlaybackShared {
    frame: Option<Vec<u8>>,
    sequence: u64,
}

pub struct Preview {
    proxy: Option<PathBuf>,
    proxy_duration: f64,
    temp_dir: tempfile::TempDir,
    generation: u64,
    rendering: bool,
    render_rx: Option<Receiver<RenderMessage>>,
    dirty_since: Option<Instant>,
    last_render_started: Option<Instant>,

    texture: Option<TextureHandle>,
    frame_rx: Option<Receiver<FrameMessage>>,
    frame_generation: u64,
    desired_frame: Option<f64>,
    loaded_frame: Option<f64>,
    pub frame_size: (u32, u32),
    pub error: Option<String>,
    pub frame_error: Option<String>,

    pub playhead: f64,
    pub playing: bool,
    play_started: Instant,
    play_start_time: f64,
    stop_flag: Arc<AtomicBool>,
    playback_shared: Arc<Mutex<PlaybackShared>>,
    playback_child: Arc<Mutex<Option<Child>>>,
    audio: Option<AudioMonitor>,
    audio_error: Option<String>,
    ffmpeg_path: PathBuf,
    last_sequence: u64,
}

impl Preview {
    pub fn new() -> Result<Self> {
        let temp_dir = tempfile::Builder::new()
            .prefix("splice-preview-")
            .tempdir()
            .context("create preview scratch directory")?;
        Ok(Self {
            proxy: None,
            proxy_duration: 0.0,
            temp_dir,
            generation: 0,
            rendering: false,
            render_rx: None,
            dirty_since: None,
            last_render_started: None,
            texture: None,
            frame_rx: None,
            frame_generation: 0,
            desired_frame: None,
            loaded_frame: None,
            frame_size: (PREVIEW_WIDTH, PREVIEW_HEIGHT),
            error: None,
            frame_error: None,
            playhead: 0.0,
            playing: false,
            play_started: Instant::now(),
            play_start_time: 0.0,
            stop_flag: Arc::new(AtomicBool::new(true)),
            playback_shared: Arc::new(Mutex::new(PlaybackShared {
                frame: None,
                sequence: 0,
            })),
            playback_child: Arc::new(Mutex::new(None)),
            audio: None,
            audio_error: None,
            ffmpeg_path: PathBuf::from("ffmpeg"),
            last_sequence: 0,
        })
    }

    pub fn texture(&self) -> Option<&TextureHandle> {
        self.texture.as_ref()
    }

    pub fn is_rendering(&self) -> bool {
        self.rendering
    }

    pub fn has_proxy(&self) -> bool {
        self.proxy.is_some()
    }

    pub fn proxy_duration(&self) -> f64 {
        self.proxy_duration
    }

    pub fn is_ready(&self) -> bool {
        self.texture.is_some() && !self.rendering
    }

    pub fn invalidate(&mut self) {
        self.stop_playback();
        self.dirty_since = Some(Instant::now());
    }

    pub fn set_playhead(&mut self, time: f64, timeline_duration: f64) {
        let time = time.clamp(0.0, timeline_duration.max(0.0));
        if (self.playhead - time).abs() > 0.0001 {
            self.playhead = time;
            if self.playing {
                self.stop_playback();
            }
            self.desired_frame = Some(time);
        }
    }

    pub fn toggle_playback(&mut self, timeline_duration: f64) {
        if self.playing {
            let elapsed = self.play_started.elapsed().as_secs_f64();
            self.playhead = (self.play_start_time + elapsed).min(timeline_duration);
            self.stop_playback();
            self.desired_frame = Some(self.playhead);
        } else {
            if timeline_duration <= 0.05 || self.proxy.is_none() {
                return;
            }
            if self.playhead >= timeline_duration - 0.03 {
                self.playhead = 0.0;
            }
            self.start_playback(timeline_duration);
        }
    }

    pub fn pause(&mut self, timeline_duration: f64) {
        if self.playing {
            let elapsed = self.play_started.elapsed().as_secs_f64();
            self.playhead = (self.play_start_time + elapsed).min(timeline_duration);
            self.stop_playback();
            self.desired_frame = Some(self.playhead);
        }
    }

    fn start_playback(&mut self, timeline_duration: f64) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        self.stop_playback();
        self.play_start_time = self.playhead;
        self.play_started = Instant::now();
        self.playing = true;
        self.playback_shared.lock().expect("playback shared").frame = None;
        self.last_sequence = 0;
        self.stop_flag = Arc::new(AtomicBool::new(false));
        self.playback_child = Arc::new(Mutex::new(None));

        match AudioMonitor::start(&self.ffmpeg_path, &proxy, self.playhead) {
            Ok(monitor) => {
                self.audio = Some(monitor);
                self.audio_error = None;
            }
            Err(error) => {
                self.audio = None;
                self.audio_error = Some(format!("audio monitoring unavailable: {error:#}"));
            }
        }

        let stop = Arc::clone(&self.stop_flag);
        let shared = Arc::clone(&self.playback_shared);
        let child_slot = Arc::clone(&self.playback_child);
        let start = self.playhead;
        let width = self.frame_size.0;
        let height = self.frame_size.1;
        let ffmpeg = self.ffmpeg_path.clone();

        std::thread::spawn(move || {
            let mut command = Command::new(&ffmpeg);
            command.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-ss",
                &format!("{start:.4}"),
                "-i",
            ]);
            command.arg(&proxy);
            command.args([
                "-an", "-vf", "fps=30", "-f", "rawvideo", "-pix_fmt", "rgba", "-",
            ]);
            command.stdout(Stdio::piped()).stderr(Stdio::null());
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(_) => return,
            };
            let mut stdout = match child.stdout.take() {
                Some(stdout) => stdout,
                None => return,
            };
            *child_slot.lock().expect("child slot") = Some(child);
            let frame_len = width as usize * height as usize * 4;
            let mut sequence = 0u64;
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let mut pixels = vec![0u8; frame_len];
                if stdout.read_exact(&mut pixels).is_err() {
                    break;
                }
                sequence += 1;
                let mut shared = shared.lock().expect("playback shared");
                shared.frame = Some(pixels);
                shared.sequence = sequence;
            }
            if let Some(mut child) = child_slot.lock().expect("child slot").take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        });
        let _ = timeline_duration;
    }

    fn stop_playback(&mut self) {
        self.playing = false;
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.playback_child.lock().expect("child slot").take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(audio) = self.audio.take() {
            audio.stop();
        }
    }

    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        project_dir: &Path,
        tools: Option<&Toolchain>,
        timeline_duration: f64,
    ) {
        self.poll_render(ctx);
        self.poll_frame(ctx);

        if timeline_duration <= 0.05 {
            self.stop_playback();
            self.playhead = 0.0;
            self.texture = None;
            return;
        }

        if let Some(dirty) = self.dirty_since {
            if !self.rendering
                && dirty.elapsed() >= Duration::from_millis(420)
                && self
                    .last_render_started
                    .map(|started| started.elapsed() >= Duration::from_millis(250))
                    .unwrap_or(true)
            {
                self.dirty_since = None;
                if let Some(tools) = tools {
                    self.start_render(project.clone(), project_dir.to_path_buf(), tools.clone());
                }
            }
        }

        if self.playing {
            let elapsed = self.play_started.elapsed().as_secs_f64();
            self.playhead = (self.play_start_time + elapsed).min(timeline_duration);
            self.pull_playback_frame(ctx);
            if self.playhead >= timeline_duration - 0.001 {
                self.playhead = timeline_duration;
                self.stop_playback();
                self.desired_frame = Some(self.playhead);
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        } else if let Some(texture) = &self.texture {
            let texture_id = texture.id();
            if let Some(desired) = self.desired_frame {
                if self
                    .loaded_frame
                    .map(|loaded| (loaded - desired).abs() > 0.002)
                    .unwrap_or(true)
                    && self.frame_rx.is_none()
                {
                    self.start_frame_load(desired);
                }
            }
            let _ = texture_id;
        }

        if self.proxy.is_some()
            && !self.playing
            && self.texture.is_none()
            && self.frame_rx.is_none()
        {
            self.start_frame_load(self.playhead.min(timeline_duration - 0.02).max(0.0));
        }
    }

    fn start_render(&mut self, project: Project, project_dir: PathBuf, tools: Toolchain) {
        self.generation += 1;
        let generation = self.generation;
        let path = self
            .temp_dir
            .path()
            .join(format!("preview-{generation}.mp4"));
        let (tx, rx) = bounded(1);
        self.render_rx = Some(rx);
        self.rendering = true;
        self.last_render_started = Some(Instant::now());
        self.error = None;
        self.ffmpeg_path = tools.ffmpeg.clone();
        self.stop_playback();

        std::thread::spawn(move || {
            let options = RenderOptions {
                width: PREVIEW_WIDTH,
                height: PREVIEW_HEIGHT,
                preview: true,
                allow_offline: true,
                level: "3.1".to_string(),
                output: path.clone(),
            };
            let message = match render_project(&project, &project_dir, &tools, &options) {
                Ok(report) => RenderMessage::Ready {
                    generation,
                    path: report.output,
                    duration: report.duration,
                },
                Err(error) => RenderMessage::Failed {
                    generation,
                    error: format!("{error:#}"),
                },
            };
            let _ = tx.send(message);
        });
    }

    fn poll_render(&mut self, _ctx: &egui::Context) {
        let Some(rx) = &self.render_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(RenderMessage::Ready {
                generation,
                path,
                duration,
            }) => {
                self.render_rx = None;
                self.rendering = false;
                if generation != self.generation {
                    let _ = std::fs::remove_file(path);
                    return;
                }
                if let Some(old) = self.proxy.replace(path.clone()) {
                    if old != path {
                        let _ = std::fs::remove_file(old);
                    }
                }
                self.proxy_duration = duration;
                self.texture = None;
                self.loaded_frame = None;
                self.desired_frame = Some(self.playhead.min(duration.max(0.01) - 0.01));
                self.frame_rx = None;
                self.start_frame_load(self.desired_frame.unwrap_or(0.0));
            }
            Ok(RenderMessage::Failed { generation, error }) => {
                self.render_rx = None;
                self.rendering = false;
                if generation == self.generation {
                    self.error = Some(error);
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.render_rx = None;
                self.rendering = false;
                self.error = Some("preview renderer stopped unexpectedly".to_string());
            }
        }
    }

    fn start_frame_load(&mut self, time: f64) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        self.frame_generation += 1;
        let generation = self.frame_generation;
        let (tx, rx) = bounded(1);
        self.frame_rx = Some(rx);
        let width = self.frame_size.0;
        let height = self.frame_size.1;
        let ffmpeg = self.ffmpeg_path.clone();
        std::thread::spawn(move || {
            let mut command = Command::new(&ffmpeg);
            command.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-ss",
                &format!("{time:.4}"),
                "-i",
            ]);
            command.arg(&proxy);
            command.args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"]);
            let output = command.output();
            let message = match output {
                Ok(output) if output.status.success() => {
                    let expected = width as usize * height as usize * 4;
                    if output.stdout.len() >= expected {
                        FrameMessage {
                            generation,
                            time,
                            width,
                            height,
                            pixels: output.stdout[..expected].to_vec(),
                            error: None,
                        }
                    } else {
                        FrameMessage {
                            generation,
                            time,
                            width,
                            height,
                            pixels: Vec::new(),
                            error: Some(format!(
                                "preview frame decoder returned {} bytes, expected {}",
                                output.stdout.len(),
                                expected
                            )),
                        }
                    }
                }
                Ok(output) => FrameMessage {
                    generation,
                    time,
                    width,
                    height,
                    pixels: Vec::new(),
                    error: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
                },
                Err(error) => FrameMessage {
                    generation,
                    time,
                    width,
                    height,
                    pixels: Vec::new(),
                    error: Some(error.to_string()),
                },
            };
            let _ = tx.send(message);
        });
    }

    fn poll_frame(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.frame_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(message) => {
                self.frame_rx = None;
                if message.generation != self.frame_generation {
                    return;
                }
                if let Some(error) = message.error {
                    self.frame_error = Some(error);
                    return;
                }
                let image = ColorImage::from_rgba_unmultiplied(
                    [message.width as usize, message.height as usize],
                    &message.pixels,
                );
                self.texture = Some(ctx.load_texture(
                    format!("preview-frame-{}", message.generation),
                    image,
                    TextureOptions::LINEAR,
                ));
                self.loaded_frame = Some(message.time);
                self.frame_error = None;
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.frame_rx = None;
            }
        }
    }

    fn pull_playback_frame(&mut self, ctx: &egui::Context) {
        let (frame, sequence) = {
            let shared = self.playback_shared.lock().expect("playback shared");
            (shared.frame.clone(), shared.sequence)
        };
        if let Some(pixels) = frame {
            if sequence != self.last_sequence {
                self.last_sequence = sequence;
                let image = ColorImage::from_rgba_unmultiplied(
                    [self.frame_size.0 as usize, self.frame_size.1 as usize],
                    &pixels,
                );
                self.texture = Some(ctx.load_texture(
                    format!("preview-play-{sequence}"),
                    image,
                    TextureOptions::LINEAR,
                ));
                self.loaded_frame = Some(self.playhead);
            }
        }
    }

    pub fn diagnostic(&self) -> Option<String> {
        self.error
            .clone()
            .or_else(|| self.frame_error.clone())
            .or_else(|| self.audio_error.clone())
    }

    pub fn finish_render_hint(&self) -> bool {
        self.rendering
            || self.proxy.is_none()
            || (self.texture.is_none() && self.frame_rx.is_some())
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.stop_playback();
    }
}

#[allow(dead_code)]
fn ffmpeg_available(tools: Option<&Toolchain>) -> Result<()> {
    match tools {
        Some(_) => Ok(()),
        None => bail!("ffmpeg unavailable"),
    }
}
