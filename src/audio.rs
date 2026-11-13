use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, SupportedStreamConfig};

#[derive(Debug, Clone)]
pub struct RecordingStats {
    pub path: PathBuf,
    pub device: String,
    pub host: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub seconds: f64,
    pub rms_dbfs: f32,
    pub peak_dbfs: f32,
    pub samples: usize,
}

impl RecordingStats {
    pub fn non_silent(&self) -> bool {
        self.rms_dbfs > -50.0 && self.peak_dbfs > -40.0
    }
}

/// A live cpal capture stream. Kept alive while the GUI records; `stop` closes
/// the stream and writes the WAV atomically through a temporary file.
pub struct Recorder {
    stream: Option<cpal::Stream>,
    _device: cpal::Device,
    buffer: Arc<Mutex<Vec<f32>>>,
    error: Arc<Mutex<Option<String>>>,
    sample_rate: u32,
    channels: u16,
    device_name: String,
    host_name: String,
    started: Instant,
    path: PathBuf,
}

impl Recorder {
    pub fn start(path: &Path) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default input device is available")?;
        let device_name = device
            .description()
            .map(|description| description.name().to_string())
            .unwrap_or_else(|_| "default input".into());
        let host_name = host.id().name().to_string();
        let config = choose_config(&device)?;
        let sample_rate = config.sample_rate();
        let channels = config.channels();
        let sample_format = config.sample_format();
        let stream_config = config.config();

        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let error_slot = Arc::new(Mutex::new(None::<String>));
        let callback_buffer = Arc::clone(&buffer);
        let error_callback = Arc::clone(&error_slot);
        let stream = build_stream(
            &device,
            &stream_config,
            sample_format,
            callback_buffer,
            error_callback,
        )?;
        stream.play().context("could not start microphone stream")?;

        Ok(Self {
            stream: Some(stream),
            _device: device,
            buffer,
            error: error_slot,
            sample_rate,
            channels,
            device_name,
            host_name,
            started: Instant::now(),
            path: path.to_path_buf(),
        })
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn host_name(&self) -> &str {
        &self.host_name
    }

    pub fn level_dbfs(&self) -> f32 {
        let buffer = self.buffer.lock().expect("microphone buffer poisoned");
        let window = (self.sample_rate as usize / 12) * self.channels as usize;
        let slice = if buffer.len() > window {
            &buffer[buffer.len() - window..]
        } else {
            &buffer[..]
        };
        rms_dbfs(slice)
    }

    pub fn stop(mut self) -> Result<RecordingStats> {
        self.stream.take();
        if let Some(error) = self.error.lock().expect("mic error poisoned").take() {
            bail!("microphone stream error: {error}");
        }
        let samples = std::mem::take(&mut *self.buffer.lock().expect("microphone buffer poisoned"));
        if samples.is_empty() {
            bail!("microphone returned no audio samples");
        }
        let frames = samples.len() as f64 / self.channels as f64;
        let seconds = frames / self.sample_rate as f64;
        let rms = rms_dbfs(&samples);
        let peak = samples
            .iter()
            .fold(0.0_f32, |max, sample| max.max(sample.abs()));
        let peak_dbfs = amplitude_dbfs(peak);
        write_wav(&self.path, self.sample_rate, self.channels, &samples)?;

        Ok(RecordingStats {
            path: self.path.clone(),
            device: self.device_name,
            host: self.host_name,
            sample_rate: self.sample_rate,
            channels: self.channels,
            seconds,
            rms_dbfs: rms,
            peak_dbfs,
            samples: samples.len(),
        })
    }
}

#[derive(Default)]
struct MonitorShared {
    samples: VecDeque<f32>,
    played: u64,
    error: Option<String>,
}

/// Plays the preview proxy soundtrack through the default cpal output. ffmpeg
/// decodes the proxy to f32 PCM from the requested start time and the output
/// callback consumes a bounded queue, so the video preview and the soundtrack
/// share one wall clock.
pub struct AudioMonitor {
    stream: Option<cpal::Stream>,
    stop: Arc<AtomicBool>,
    decoder: Option<JoinHandle<()>>,
    child: Arc<Mutex<Option<Child>>>,
    shared: Arc<Mutex<MonitorShared>>,
    channels: u16,
    ffmpeg: PathBuf,
    source: PathBuf,
    last_rebuild: Option<Instant>,
    rebuilds: u32,
}

impl AudioMonitor {
    pub fn start(ffmpeg: &Path, source: &Path, start_seconds: f64) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default audio output device is available")?;
        let supported = device
            .default_output_config()
            .context("default output device has no configuration")?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels();
        let sample_format = supported.sample_format();
        let config = supported.config();

        let shared = Arc::new(Mutex::new(MonitorShared::default()));
        let stream = build_output_stream(&device, &config, sample_format, Arc::clone(&shared))?;
        stream.play().context("could not start audio monitoring")?;

        let stop = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(None));
        let decoder = spawn_decoder(
            ffmpeg,
            source,
            start_seconds,
            sample_rate,
            channels,
            Arc::clone(&shared),
            Arc::clone(&stop),
            Arc::clone(&child),
        );

        Ok(Self {
            stream: Some(stream),
            stop,
            decoder: Some(decoder),
            child,
            shared,
            channels,
            ffmpeg: ffmpeg.to_path_buf(),
            source: source.to_path_buf(),
            last_rebuild: None,
            rebuilds: 0,
        })
    }

    /// Frames actually handed to the output device. Used by the audio gate.
    pub fn played_frames(&self) -> u64 {
        let shared = self.shared.lock().expect("audio monitor poisoned");
        shared.played / self.channels as u64
    }

    pub fn diagnostic(&self) -> Option<String> {
        self.shared
            .lock()
            .expect("audio monitor poisoned")
            .error
            .clone()
    }
    /// Reopens the default output and resumes decoding at `resume_seconds`
    /// when the cpal stream reports an error (for example after a device
    /// hot-plug). Rate-limited so a permanently unavailable device does not
    /// spin.
    pub fn supervise(&mut self, resume_seconds: f64) -> bool {
        let has_error = self
            .shared
            .lock()
            .expect("audio monitor poisoned")
            .error
            .is_some();
        if !has_error {
            return false;
        }
        let now = Instant::now();
        if self
            .last_rebuild
            .is_some_and(|last| now.duration_since(last) < Duration::from_millis(750))
        {
            return false;
        }
        self.shutdown();
        let rebuilds = self.rebuilds.saturating_add(1);
        match AudioMonitor::start(&self.ffmpeg, &self.source, resume_seconds.max(0.0)) {
            Ok(mut fresh) => {
                fresh.rebuilds = rebuilds;
                fresh.last_rebuild = Some(now);
                *self = fresh;
                true
            }
            Err(error) => {
                let mut shared = self.shared.lock().expect("audio monitor poisoned");
                shared.samples.clear();
                shared.error = Some(format!("audio output recovery failed: {error:#}"));
                self.last_rebuild = Some(now);
                false
            }
        }
    }

    pub fn rebuilds(&self) -> u32 {
        self.rebuilds
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.stream.take();
        if let Some(mut child) = self.child.lock().expect("audio child poisoned").take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = self.decoder.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AudioMonitor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_decoder(
    ffmpeg: &Path,
    source: &Path,
    start_seconds: f64,
    sample_rate: u32,
    channels: u16,
    shared: Arc<Mutex<MonitorShared>>,
    stop: Arc<AtomicBool>,
    child_slot: Arc<Mutex<Option<Child>>>,
) -> JoinHandle<()> {
    let ffmpeg = ffmpeg.to_path_buf();
    let source = source.to_path_buf();
    std::thread::spawn(move || {
        let mut command = Command::new(&ffmpeg);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-ss",
            &format!("{start_seconds:.4}"),
            "-i",
        ]);
        command.arg(&source);
        command.args([
            "-vn",
            "-f",
            "f32le",
            "-ac",
            &channels.to_string(),
            "-ar",
            &sample_rate.to_string(),
            "-",
        ]);
        command.stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                shared.lock().expect("audio monitor poisoned").error =
                    Some(format!("could not start ffmpeg for audio: {error}"));
                return;
            }
        };
        let mut stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                shared.lock().expect("audio monitor poisoned").error =
                    Some("ffmpeg produced no audio pipe".to_string());
                return;
            }
        };
        *child_slot.lock().expect("audio child poisoned") = Some(child);

        let frame_bytes = 4 * channels as usize;
        let max_buffered = (sample_rate as usize * channels as usize * 2).max(frame_bytes);
        let mut pending: Vec<u8> = Vec::new();
        let mut buffer = [0u8; 8192];

        'decode: loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            // Backpressure: never buffer more than two seconds of audio.
            loop {
                let len = shared.lock().expect("audio monitor poisoned").samples.len();
                if len < max_buffered || stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let read = match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(_) => break,
            };
            pending.extend_from_slice(&buffer[..read]);
            let usable = pending.len() - pending.len() % frame_bytes;
            if usable == 0 {
                continue;
            }
            let samples: Vec<f32> = pending[..usable]
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            pending.drain(..usable);

            let mut offset = 0;
            while offset < samples.len() {
                if stop.load(Ordering::Relaxed) {
                    break 'decode;
                }
                let space = {
                    let shared = shared.lock().expect("audio monitor poisoned");
                    max_buffered.saturating_sub(shared.samples.len())
                };
                if space == 0 {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                let take = space.min(samples.len() - offset);
                let mut shared = shared.lock().expect("audio monitor poisoned");
                shared.samples.extend(&samples[offset..offset + take]);
                offset += take;
            }
        }

        if let Some(mut child) = child_slot.lock().expect("audio child poisoned").take() {
            let _ = child.wait();
        }
    })
}

fn build_output_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    shared: Arc<Mutex<MonitorShared>>,
) -> Result<cpal::Stream> {
    macro_rules! make {
        ($ty:ty) => {
            build_output_typed::<$ty>(device, config, Arc::clone(&shared))
        };
    }
    match sample_format {
        SampleFormat::F32 => make!(f32),
        SampleFormat::F64 => make!(f64),
        SampleFormat::I8 => make!(i8),
        SampleFormat::I16 => make!(i16),
        SampleFormat::I32 => make!(i32),
        SampleFormat::I64 => make!(i64),
        SampleFormat::U8 => make!(u8),
        SampleFormat::U16 => make!(u16),
        SampleFormat::U32 => make!(u32),
        SampleFormat::U64 => make!(u64),
        other => bail!("unsupported audio output sample format {other:?}"),
    }
}

fn build_output_typed<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Mutex<MonitorShared>>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let error_slot = Arc::clone(&shared);
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _info| {
                let mut shared = shared.lock().expect("audio monitor poisoned");
                if shared.samples.len() >= data.len() {
                    for sample in data.iter_mut() {
                        let value = shared.samples.pop_front().expect("length checked");
                        *sample = T::from_sample(value);
                    }
                    shared.played += data.len() as u64;
                } else {
                    for sample in data.iter_mut() {
                        *sample = T::EQUILIBRIUM;
                    }
                }
            },
            move |error| {
                if let Ok(mut shared) = error_slot.lock() {
                    shared.error = Some(format!("audio output error: {error}"));
                }
            },
            None,
        )
        .context("could not open default audio output")
}

pub fn record_for(seconds: f64, path: &Path) -> Result<RecordingStats> {
    let recorder = Recorder::start(path)?;
    std::thread::sleep(Duration::from_secs_f64(seconds.max(0.2)));
    recorder.stop()
}

pub fn describe_default_input() -> Result<String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("no default input device is available")?;
    let name = device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_else(|_| "default input".into());
    let config = choose_config(&device)?;
    Ok(format!(
        "{} ({}) {} Hz, {} ch, {:?}",
        name,
        host.id().name(),
        config.sample_rate(),
        config.channels(),
        config.sample_format()
    ))
}

fn choose_config(device: &cpal::Device) -> Result<SupportedStreamConfig> {
    let default = device
        .default_input_config()
        .context("default input device has no input configuration")?;

    if let Ok(ranges) = device.supported_input_configs() {
        let mut best: Option<(u8, SupportedStreamConfig)> = None;
        for range in ranges {
            if range.min_sample_rate() <= 48_000 && range.max_sample_rate() >= 48_000 {
                let format_rank = match range.sample_format() {
                    SampleFormat::F32 => 0,
                    SampleFormat::I16 => 1,
                    SampleFormat::U16 => 2,
                    SampleFormat::I32 => 3,
                    SampleFormat::F64 => 4,
                    SampleFormat::I8 => 5,
                    SampleFormat::U8 => 6,
                    _ => 9,
                };
                let channel_rank = if range.channels() == 2 {
                    0
                } else if range.channels() == 1 {
                    1
                } else {
                    2
                };
                let rank = format_rank + channel_rank;
                let candidate = range.with_sample_rate(48_000);
                if best
                    .as_ref()
                    .map(|(best_rank, _)| rank < *best_rank)
                    .unwrap_or(true)
                {
                    best = Some((rank, candidate));
                }
            }
        }
        if let Some((_, config)) = best {
            return Ok(config);
        }
    }

    Ok(default)
}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    buffer: Arc<Mutex<Vec<f32>>>,
    error_slot: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream> {
    macro_rules! make {
        ($ty:ty) => {
            build_typed::<$ty>(device, config, buffer, error_slot)
        };
    }
    match sample_format {
        SampleFormat::F32 => make!(f32),
        SampleFormat::F64 => make!(f64),
        SampleFormat::I8 => make!(i8),
        SampleFormat::I16 => make!(i16),
        SampleFormat::I32 => make!(i32),
        SampleFormat::I64 => make!(i64),
        SampleFormat::U8 => make!(u8),
        SampleFormat::U16 => make!(u16),
        SampleFormat::U32 => make!(u32),
        SampleFormat::U64 => make!(u64),
        other => bail!("unsupported microphone sample format {other:?}"),
    }
}

fn build_typed<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    error_slot: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let stream = device
        .build_input_stream(
            *config,
            move |data: &[T], _info| {
                let mut buffer = buffer.lock().expect("microphone buffer poisoned");
                buffer.extend(data.iter().map(|sample| f32::from_sample(*sample)));
            },
            move |error| {
                if let Ok(mut slot) = error_slot.lock() {
                    *slot = Some(error.to_string());
                }
            },
            None,
        )
        .context("could not open default microphone")?;
    Ok(stream)
}

/// Writes the WAV beside its destination and renames it into place, so a
/// crash or full disk never leaves a truncated recording at `path`.
fn write_wav(path: &Path, sample_rate: u32, channels: u16, samples: &[f32]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("could not create {}", parent.display()))?;
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("could not create a temporary file in {}", parent.display()))?;
    {
        let buffered = std::io::BufWriter::new(temp.as_file_mut());
        let mut writer = hound::WavWriter::new(buffered, spec)
            .with_context(|| format!("could not write {}", path.display()))?;
        for sample in samples {
            let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            writer
                .write_sample(value)
                .context("write microphone sample")?;
        }
        writer.finalize().context("finalize microphone WAV")?;
    }
    temp.as_file_mut().sync_all().context("fsync recording")?;
    temp.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("could not move the recording into {}", path.display()))?;
    Ok(())
}

fn rms_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -120.0;
    }
    let sum = samples
        .iter()
        .map(|sample| (*sample as f64) * (*sample as f64))
        .sum::<f64>();
    let rms = (sum / samples.len() as f64).sqrt() as f32;
    amplitude_dbfs(rms)
}

fn amplitude_dbfs(amplitude: f32) -> f32 {
    if amplitude <= 1e-7 {
        -140.0
    } else {
        20.0 * amplitude.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_is_written_atomically_and_reads_back() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("nested").join("take.wav");
        let samples: Vec<f32> = (0..4800).map(|i| (i as f32 / 4800.0) - 0.5).collect();
        write_wav(&path, 48_000, 2, &samples).expect("write");

        let reader = hound::WavReader::open(&path).expect("open");
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 48_000);
        assert_eq!(reader.len() as usize, samples.len());
        let entries: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1, "temporary file left behind: {entries:?}");
    }

    /// Hardware gate: exercises the real default output device. Kept ignored so
    /// headless machines can still run the deterministic suite; run it with
    /// `cargo test --lib audio::tests::audio_monitor_plays_decoded_audio -- --ignored`.
    #[test]
    #[ignore = "requires a default audio output device and system ffmpeg"]
    fn audio_monitor_plays_decoded_audio() {
        let host = cpal::default_host();
        if host.default_output_device().is_none() {
            eprintln!("no default output device; skipping");
            return;
        }
        let dir = tempfile::tempdir().expect("temp dir");
        let tone = dir.path().join("tone.wav");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=2",
                "-ac",
                "2",
            ])
            .arg(&tone)
            .status()
            .expect("run ffmpeg");
        assert!(status.success(), "could not generate the tone fixture");

        let monitor =
            AudioMonitor::start(Path::new("ffmpeg"), &tone, 0.0).expect("start audio monitoring");
        std::thread::sleep(Duration::from_millis(600));
        let frames = monitor.played_frames();
        let diagnostic = monitor.diagnostic();
        monitor.stop();
        assert!(frames > 0, "output callback consumed no audio frames");
        assert!(diagnostic.is_none(), "audio output reported {diagnostic:?}");
    }

    /// Hardware gate for recovery: inject a stream error and verify the
    /// supervisor reopens the device and resumes feeding frames.
    #[test]
    #[ignore = "requires a default audio output device and system ffmpeg"]
    fn audio_monitor_rebuilds_after_output_error() {
        let host = cpal::default_host();
        if host.default_output_device().is_none() {
            eprintln!("no default output device; skipping");
            return;
        }
        let dir = tempfile::tempdir().expect("temp dir");
        let tone = dir.path().join("tone.wav");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=2",
                "-ac",
                "2",
            ])
            .arg(&tone)
            .status()
            .expect("run ffmpeg");
        assert!(status.success(), "could not generate the tone fixture");

        let mut monitor =
            AudioMonitor::start(Path::new("ffmpeg"), &tone, 0.0).expect("start audio monitoring");
        std::thread::sleep(Duration::from_millis(300));
        monitor.shared.lock().expect("audio monitor poisoned").error =
            Some("simulated device loss".to_string());
        assert!(
            monitor.supervise(0.3),
            "supervisor should rebuild after an output error"
        );
        assert_eq!(monitor.rebuilds(), 1);
        std::thread::sleep(Duration::from_millis(500));
        let frames = monitor.played_frames();
        let diagnostic = monitor.diagnostic();
        monitor.stop();
        assert!(frames > 0, "rebuilt output callback consumed no frames");
        assert!(
            diagnostic.is_none(),
            "rebuilt output reported {diagnostic:?}"
        );
    }
}
