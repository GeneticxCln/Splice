use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::ffmpeg::Toolchain;
use crate::filters::{ffmpeg_color, frame_filter, push_effect};
use crate::model::{
    Clip, MediaAsset, MediaKind, Project, Track, TrackKind, TransitionKind, resolve_media_path,
};
use crate::runner::{self, Cancelled, RenderControl};
use crate::titles::{escape_filter_path, write_ass};

const OFFLINE_FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
];

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    pub preview: bool,
    /// Preview renders missing clips as labelled offline placeholders. Export
    /// validates first and never sets this.
    pub allow_offline: bool,
    pub level: String,
    pub output: PathBuf,
    /// Cancel and progress handle for the ffmpeg run.
    pub control: RenderControl,
}

#[derive(Debug, Clone)]
pub struct RenderReport {
    pub output: PathBuf,
    pub duration: f64,
    pub command: String,
    /// Size of the generated filter graph; it is passed to ffmpeg by file, so
    /// it may exceed the kernel's per-argument limit.
    pub graph_bytes: usize,
}

/// Longest graph text echoed in an error message.
const GRAPH_ERROR_CHARS: usize = 4000;

struct InputSpec {
    path: PathBuf,
    image: bool,
    /// Source window (start, length) in seconds. ffmpeg seeks straight to
    /// the start instead of decoding the file from the beginning.
    window: Option<(f64, f64)>,
}

/// The part of the source a clip plays, clamped to the file's duration.
fn source_window(clip: &Clip, asset: &MediaAsset) -> (f64, f64) {
    let limit = if asset.duration > 0.0 {
        asset.duration
    } else {
        clip.out_point
    };
    let start = clip.in_point.clamp(0.0, (limit - 0.02).max(0.0));
    let end = clip.out_point.min(limit).max(start + 0.01);
    (start, end)
}

struct Visual {
    label: String,
    start: f64,
    duration: f64,
    transition: Option<TransitionKind>,
}

/// Renders the full project graph at the requested size. Missing files are a
/// hard error unless allow_offline is set (preview only).
pub fn render_project(
    project: &Project,
    project_dir: &Path,
    tools: &Toolchain,
    options: &RenderOptions,
) -> Result<RenderReport> {
    tools.require_encoders()?;
    let duration = project.timeline_duration();
    if !duration.is_finite() || duration <= 0.01 {
        bail!("the timeline is empty; add at least one clip before exporting or previewing");
    }
    if !options.allow_offline {
        let missing: Vec<String> = project
            .missing_assets(project_dir)
            .iter()
            .map(|asset| {
                format!(
                    "{} ({})",
                    asset.name,
                    resolve_media_path(project_dir, asset).display()
                )
            })
            .collect();
        if !missing.is_empty() {
            bail!("cannot render: missing media: {}", missing.join(", "));
        }
    }

    let width = even(options.width.max(16));
    let height = even(options.height.max(16));
    let timeline = (duration * 1000.0).round() / 1000.0;

    let mut inputs: Vec<InputSpec> = Vec::new();
    let mut input_for_clip: HashMap<String, usize> = HashMap::new();
    for track in &project.tracks {
        for clip in &track.clips {
            let Some(media_id) = clip.media_id.as_deref() else {
                continue;
            };
            let Some(asset) = project.media_by_id(media_id) else {
                continue;
            };
            let path = resolve_media_path(project_dir, asset);
            if !path.is_file() {
                if !options.allow_offline {
                    bail!("cannot render: missing media {}", path.display());
                }
                continue;
            }
            let index = inputs.len();
            let image = asset.kind == MediaKind::Image;
            let window = if image {
                None
            } else {
                let (start, end) = source_window(clip, asset);
                Some((start, end - start))
            };
            inputs.push(InputSpec {
                path,
                image,
                window,
            });
            input_for_clip.insert(clip.id.clone(), index);
        }
    }

    let silence_index = inputs.len();
    inputs.push(InputSpec {
        path: PathBuf::new(),
        image: false,
        window: None,
    });

    let mut filters: Vec<String> = Vec::new();
    let mut next_label = 0usize;
    let mut used_labels = LabelAllocator::default();

    let primary = project
        .primary_video_track()
        .cloned()
        .unwrap_or_else(|| Track {
            id: "missing-primary".into(),
            name: "Video 1".into(),
            kind: TrackKind::Video,
            muted: false,
            clips: Vec::new(),
        });

    let mut visuals: Vec<Visual> = Vec::new();
    for clip in &primary.clips {
        let (label, online_duration) = prepare_visual(
            project,
            project_dir,
            clip,
            input_for_clip.get(&clip.id).copied(),
            &width,
            &height,
            options.allow_offline,
            &mut filters,
            &mut next_label,
            &mut used_labels,
        )?;
        let duration = online_duration.unwrap_or(clip.duration());
        let label = apply_rgba_motion(
            label,
            clip,
            duration,
            width,
            height,
            Some(&project.settings.background),
            &mut filters,
            &mut used_labels,
        );
        visuals.push(Visual {
            label,
            start: clip.start,
            duration,
            transition: clip.transition.as_ref().map(|transition| transition.kind),
        });
    }
    visuals.sort_by(|a, b| a.start.total_cmp(&b.start));

    let (base_label, base_end) = build_primary(
        &visuals,
        &mut filters,
        width,
        height,
        timeline,
        &mut next_label,
    )?;

    // Extend the base canvas so overlays and titles cannot truncate it.
    let (mut composed, _) = trim_and_pad(
        &base_label,
        base_end,
        timeline,
        width,
        height,
        &mut filters,
        &mut next_label,
    )?;

    // Video overlays are real multi-track composites.
    for track in &project.tracks {
        if track.kind != TrackKind::Overlay {
            continue;
        }
        let mut overlays: Vec<(&Clip, &MediaAsset, PathBuf)> = Vec::new();
        for clip in &track.clips {
            if clip.title.is_some() {
                continue;
            }
            let Some(media_id) = clip.media_id.as_deref() else {
                continue;
            };
            let Some(asset) = project.media_by_id(media_id) else {
                continue;
            };
            let path = resolve_media_path(project_dir, asset);
            if !path.is_file() {
                continue;
            }
            if !input_for_clip.contains_key(&clip.id) {
                continue;
            }
            overlays.push((clip, asset, path));
        }
        overlays.sort_by(|a, b| a.0.start.total_cmp(&b.0.start));
        for (clip, _asset, _path) in overlays {
            let Some(input_index) = input_for_clip.get(&clip.id).copied() else {
                continue;
            };
            let (overlay_label, _) = prepare_visual(
                project,
                project_dir,
                clip,
                Some(input_index),
                &width,
                &height,
                options.allow_offline,
                &mut filters,
                &mut next_label,
                &mut used_labels,
            )?;
            let overlay_label = apply_rgba_motion(
                overlay_label,
                clip,
                clip.duration(),
                width,
                height,
                None,
                &mut filters,
                &mut used_labels,
            );
            let padded = used_labels.next("ov");
            let start_ms = (clip.start * 1000.0).round().max(0.0) as u64;
            if start_ms > 0 {
                filters.push(format!(
                    "[{overlay_label}]format=rgba,tpad=start_duration={:.3}:start_mode=add:color=black@0.0,format=yuva420p[{padded}]",
                    clip.start
                ));
            } else {
                filters.push(format!("[{overlay_label}]format=yuva420p[{padded}]"));
            }
            let next = used_labels.next("comp");
            let end = clip.start + clip.duration();
            filters.push(format!(
                "[{composed}][{padded}]overlay=x=0:y=0:enable='between(t,{:.3},{:.3})':eof_action=pass:shortest=0,format=yuv420p[{next}]",
                clip.start, end
            ));
            composed = next;
        }
    }

    // Titles are rendered by libass from the same ASS file for preview/export.
    let temp_dir = tempfile::Builder::new()
        .prefix("splice-titles-")
        .tempdir()
        .context("create title overlay directory")?;
    if write_ass(project, width, height, &temp_dir.path().join("titles.ass"))? {
        let ass_path = temp_dir.path().join("titles.ass");
        let escaped = escape_filter_path(&ass_path);
        let fontsdir = Path::new("/usr/share/fonts");
        let next = used_labels.next("titled");
        if fontsdir.is_dir() {
            let fonts = escape_filter_path(fontsdir);
            filters.push(format!(
                "[{composed}]ass=filename='{escaped}':fontsdir='{fonts}'[{next}]"
            ));
        } else {
            filters.push(format!("[{composed}]ass=filename='{escaped}'[{next}]"));
        }
        composed = next;
    }

    let final_label = used_labels.next("vout");
    filters.push(format!(
        "[{composed}]trim=duration={timeline:.3},setpts=PTS-STARTPTS,format=yuv420p[{final_label}]"
    ));

    let audio_out = build_audio_mix(
        project,
        &input_for_clip,
        silence_index,
        timeline,
        &mut used_labels,
        &mut filters,
    );

    let graph = filters.join(";");
    let graph_path = temp_dir.path().join("graph.txt");
    std::fs::write(&graph_path, &graph)
        .with_context(|| format!("could not write filter graph {}", graph_path.display()))?;

    let mut command = Command::new(&tools.ffmpeg);
    command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    command.args(["-progress", "pipe:1", "-nostats"]);
    for input in &inputs {
        if input.image {
            command.args(["-loop", "1", "-framerate", "30"]);
        }
        if let Some((start, length)) = input.window {
            command.args(["-ss", &format!("{start:.4}"), "-t", &format!("{length:.4}")]);
        }
        if input.path.as_os_str().is_empty() {
            command.args([
                "-f",
                "lavfi",
                "-i",
                "anullsrc=channel_layout=stereo:sample_rate=48000",
            ]);
        } else {
            // Project files can be shared, so a media path must only ever be
            // opened as a local file, never as a URL or other protocol.
            command.args(["-protocol_whitelist", "file"]);
            command.arg("-i").arg(&input.path);
        }
    }
    command.args(tools.graph_file.args(&graph_path));
    command.args(["-map", &format!("[{final_label}]")]);
    command.args(["-map", &format!("[{audio_out}]")]);
    command.args(["-t", &format!("{timeline:.3}")]);
    command.args(["-r", "30"]);
    command.args(["-c:v", "libx264"]);
    if options.preview {
        command.args(["-preset", "ultrafast", "-crf", "28", "-level", "3.1"]);
    } else {
        command.args(["-preset", "medium", "-crf", "20", "-profile:v", "high"]);
        command.args(["-level", &options.level]);
    }
    command.args([
        "-pix_fmt",
        "yuv420p",
        "-g",
        "60",
        "-keyint_min",
        "30",
        "-sc_threshold",
        "0",
    ]);
    command.args([
        "-c:a",
        "aac",
        "-b:a",
        if options.preview { "128k" } else { "192k" },
    ]);
    command.args(["-ar", "48000", "-ac", "2"]);
    command.args(["-movflags", "+faststart"]);
    command.arg(&options.output);

    let display = command_display(&command);
    if let Err(error) = runner::run(command, &options.control) {
        let _ = std::fs::remove_file(&options.output);
        if error.downcast_ref::<Cancelled>().is_some() {
            return Err(error);
        }
        let shown: String = graph.chars().take(GRAPH_ERROR_CHARS).collect();
        let more = if graph.chars().count() > GRAPH_ERROR_CHARS {
            "…"
        } else {
            ""
        };
        bail!("ffmpeg failed ({error:#})\n{display}\nfilter graph: {shown}{more}");
    }
    if !options.output.is_file() {
        bail!("ffmpeg exited successfully but produced no output file");
    }
    Ok(RenderReport {
        output: options.output.clone(),
        duration: timeline,
        command: display,
        graph_bytes: graph.len(),
    })
}

/// Adds the audio mix to `filters` and returns the label of the mixed track.
/// Every unmuted clip with audio is trimmed, faded, delayed to its timeline
/// position and summed; a silent input fills in when nothing plays.
fn build_audio_mix(
    project: &Project,
    input_for_clip: &HashMap<String, usize>,
    silence_index: usize,
    timeline: f64,
    used_labels: &mut LabelAllocator,
    filters: &mut Vec<String>,
) -> String {
    let mut audio_labels: Vec<String> = Vec::new();
    for track in &project.tracks {
        for clip in &track.clips {
            if track.muted || clip.audio.muted {
                continue;
            }
            let Some(media_id) = clip.media_id.as_deref() else {
                continue;
            };
            let Some(asset) = project.media_by_id(media_id) else {
                continue;
            };
            if !asset.has_audio {
                continue;
            }
            let Some(input_index) = input_for_clip.get(&clip.id).copied() else {
                continue;
            };
            if clip.duration() <= 0.001 {
                continue;
            }
            let label = used_labels.next("a");
            let mut chain = String::new();
            // The input is already seeked to the clip's in-point.
            let (source_start, source_end) = source_window(clip, asset);
            let window = source_end - source_start;
            let duration = clip.duration();
            let _ = write!(
                chain,
                "[{input_index}:a]atrim=duration={window:.4},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad=whole_dur={duration:.4},atrim=duration={duration:.4}"
            );
            let volume = clip.audio.volume.clamp(0.0, 2.0);
            let _ = write!(chain, ",volume={volume:.4}");
            let fade_in = clip.audio.fade_in.clamp(0.0, duration);
            if fade_in > 0.001 {
                let _ = write!(chain, ",afade=t=in:st=0:d={fade_in:.4}");
            }
            let fade_out = clip.audio.fade_out.clamp(0.0, duration);
            if fade_out > 0.001 {
                let start = (duration - fade_out).max(0.0);
                let _ = write!(chain, ",afade=t=out:st={start:.4}:d={fade_out:.4}");
            }
            let delay_ms = (clip.start * 1000.0).round().max(0.0) as u64;
            let _ = write!(chain, ",adelay={delay_ms}|{delay_ms}[{label}]");
            filters.push(chain);
            audio_labels.push(label);
        }
    }

    let silence_input = format!("{silence_index}:a");
    let audio_out = used_labels.next("aout");
    match audio_labels.len() {
        0 => {
            filters.push(format!(
                "[{silence_input}]atrim=duration={timeline:.3},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[{audio_out}]"
            ));
        }
        1 => {
            filters.push(format!(
                "[{}]alimiter=limit=0.95,apad,atrim=duration={timeline:.3},asetpts=PTS-STARTPTS[{audio_out}]",
                audio_labels[0]
            ));
        }
        count => {
            let mut chain = String::new();
            for label in &audio_labels {
                let _ = write!(chain, "[{label}]");
            }
            let _ = write!(
                chain,
                "amix=inputs={count}:duration=longest:normalize=0:dropout_transition=0,alimiter=limit=0.95,apad,atrim=duration={timeline:.3},asetpts=PTS-STARTPTS[{audio_out}]"
            );
            filters.push(chain);
        }
    }
    audio_out
}

/// Applies rotation and opacity in a real RGBA pass. Primary clips are
/// composited over the project background; overlays keep their alpha for the
/// later multi-track composite.
#[allow(clippy::too_many_arguments)]
fn apply_rgba_motion(
    label: String,
    clip: &Clip,
    duration: f64,
    width: u32,
    height: u32,
    background: Option<&str>,
    filters: &mut Vec<String>,
    labels: &mut LabelAllocator,
) -> String {
    let duration = duration.max(0.01);
    let rotation = clip.motion.rotation_expression(duration);
    let opacity = clip.motion.opacity_expression(duration);
    if rotation.is_none() && opacity.is_none() {
        return label;
    }

    let mut chain = format!("[{label}]format=rgba");
    if let Some(rotation) = rotation {
        let _ = write!(
            chain,
            ",rotate=angle='{rotation}':fillcolor=black@0:ow=iw:oh=ih"
        );
    }
    if let Some(opacity) = opacity {
        let _ = write!(
            chain,
            ",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({opacity})'"
        );
    }

    let motion_label = labels.next("motion");
    filters.push(format!("{chain}[{motion_label}]"));
    // Composite only after rotation and alpha reduction. Putting the opaque
    // background in the motion chain would fade the background with the clip.
    if let Some(background) = background {
        let background_label = labels.next("motionbg");
        filters.push(format!(
            "color=c={}:s={width}x{height}:r=30:d={duration:.4},format=yuva420p[{background_label}]",
            ffmpeg_color(background)
        ));
        let output = labels.next("motioncomp");
        filters.push(format!(
            "[{background_label}][{motion_label}]overlay=x=0:y=0:shortest=1:format=auto,format=yuv420p[{output}]"
        ));
        output
    } else {
        motion_label
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_visual(
    project: &Project,
    project_dir: &Path,
    clip: &Clip,
    input_index: Option<usize>,
    width: &u32,
    height: &u32,
    allow_offline: bool,
    filters: &mut Vec<String>,
    next_label: &mut usize,
    labels: &mut LabelAllocator,
) -> Result<(String, Option<f64>)> {
    let label = labels.next("v");
    let media_id = clip.media_id.as_deref();
    let asset = media_id.and_then(|id| project.media_by_id(id));
    let online = match asset {
        Some(asset) => resolve_media_path(project_dir, asset).is_file(),
        None => false,
    };

    if input_index.is_none() || !online || asset.is_none() {
        if !allow_offline {
            bail!("cannot render clip {}: source media is offline", clip.name);
        }
        push_offline_placeholder(
            filters,
            &label,
            width,
            height,
            clip.duration(),
            clip.title.is_some(),
        );
        *next_label += 1;
        return Ok((label, None));
    }

    let asset = asset.expect("asset checked");
    let input = input_index.expect("input checked");
    let mut chain = String::new();
    let requested = clip.duration().max(0.01);
    let framing = frame_filter(
        *width,
        *height,
        project.settings.framing,
        &project.settings.background,
    );

    if asset.kind == MediaKind::Image {
        let _ = write!(
            chain,
            "[{input}:v]trim=duration={requested:.4},setpts=PTS-STARTPTS,{framing},setsar=1,fps=30"
        );
    } else {
        let (start, end) = source_window(clip, asset);
        let available = (end - start).max(0.01);
        let extra = (requested - available).max(0.0);
        // The input is already seeked to `start`, so only the length is cut.
        let _ = write!(
            chain,
            "[{input}:v]trim=duration={available:.4},setpts=PTS-STARTPTS,{framing},setsar=1,fps=30"
        );
        if extra > 0.01 {
            let _ = write!(chain, ",tpad=stop_mode=clone:stop_duration={extra:.4}");
        }
    }

    for effect in &clip.effects {
        push_effect(&mut chain, effect, requested, *width, *height);
    }

    if let Some((zoom, pan_x, pan_y)) = clip.motion.zoompan_expressions(requested) {
        let _ = write!(
            chain,
            ",zoompan=z='{zoom}':x='{pan_x}':y='{pan_y}':d=1:s={width}x{height}:fps=30"
        );
    }

    let _ = write!(
        chain,
        ",format=yuv420p,settb=AVTB,setpts=PTS-STARTPTS[{label}]"
    );
    filters.push(chain);
    Ok((label, Some(requested)))
}

fn push_offline_placeholder(
    filters: &mut Vec<String>,
    label: &str,
    width: &u32,
    height: &u32,
    duration: f64,
    title_slot: bool,
) {
    let font = OFFLINE_FONT_CANDIDATES
        .iter()
        .find(|path| Path::new(path).is_file());
    let mut chain = format!(
        "color=c=0x11151b:s={width}x{height}:r=30:d={:.3},format=yuv420p",
        duration.max(0.01)
    );
    if !title_slot {
        if let Some(font) = font {
            let _ = write!(
                chain,
                ",drawtext=fontfile='{}':text='MEDIA OFFLINE':fontcolor=0xFF6B7A:fontsize=42:x=(w-text_w)/2:y=(h-text_h)/2:borderw=2:bordercolor=0x0D1016",
                escape_filter_path(Path::new(font))
            );
        }
    }
    let _ = write!(chain, ",settb=AVTB[{label}]");
    filters.push(chain);
}

fn build_primary(
    visuals: &[Visual],
    filters: &mut Vec<String>,
    width: u32,
    height: u32,
    timeline: f64,
    next_label: &mut usize,
) -> Result<(String, f64)> {
    if visuals.is_empty() {
        let label = format!("base{}", next_label_suffix(next_label));
        filters.push(format!(
            "color=c=0x101318:s={width}x{height}:r=30:d={timeline:.3},format=yuv420p,settb=AVTB[{label}]"
        ));
        return Ok((label, timeline));
    }

    let mut current: Option<(String, f64)> = None;
    let mut last_end = 0.0f64;

    for visual in visuals {
        if visual.start > last_end + 0.001 {
            let gap = visual.start - last_end;
            let gap_label = format!("gap{}", next_label_suffix(next_label));
            filters.push(format!(
                "color=c=0x101318:s={width}x{height}:r=30:d={gap:.4},format=yuv420p,settb=AVTB[{gap_label}]"
            ));
            match current.take() {
                Some((label, _end)) => {
                    let merged = format!("join{}", next_label_suffix(next_label));
                    filters.push(format!(
                        "[{label}][{gap_label}]concat=n=2:v=1:a=0,format=yuv420p,settb=AVTB[{merged}]"
                    ));
                    current = Some((merged, visual.start));
                }
                None => {
                    current = Some((gap_label, visual.start));
                }
            }
        }

        match current.take() {
            None => {
                current = Some((visual.label.clone(), visual.start + visual.duration));
                last_end = visual.start + visual.duration;
            }
            Some((label, end)) => {
                if visual.start < end - 0.03 {
                    let transition = end - visual.start;
                    let overlap = transition
                        .min(visual.duration * 0.9)
                        .min((end - visual.start).max(0.0));
                    if overlap > 0.03 {
                        let kind = visual
                            .transition
                            .unwrap_or(TransitionKind::Fade)
                            .xfade_name();
                        let joined = format!("xf{}", next_label_suffix(next_label));
                        filters.push(format!(
                            "[{label}][{}]xfade=transition={kind}:duration={overlap:.4}:offset={:.4},format=yuv420p,settb=AVTB[{joined}]",
                            visual.label, visual.start
                        ));
                        current = Some((joined, visual.start + visual.duration));
                        last_end = visual.start + visual.duration;
                    } else {
                        let joined = format!("cat{}", next_label_suffix(next_label));
                        filters.push(format!(
                            "[{label}][{}]concat=n=2:v=1:a=0,format=yuv420p,settb=AVTB[{joined}]",
                            visual.label
                        ));
                        current = Some((joined, end + visual.duration));
                        last_end = end + visual.duration;
                    }
                } else {
                    let joined = format!("cat{}", next_label_suffix(next_label));
                    filters.push(format!(
                        "[{label}][{}]concat=n=2:v=1:a=0,format=yuv420p,settb=AVTB[{joined}]",
                        visual.label
                    ));
                    current = Some((joined, end + visual.duration));
                    last_end = end + visual.duration;
                }
            }
        }
    }

    current.ok_or_else(|| anyhow::anyhow!("failed to build primary video chain"))
}

fn trim_and_pad(
    label: &str,
    end: f64,
    timeline: f64,
    width: u32,
    height: u32,
    filters: &mut Vec<String>,
    next_label: &mut usize,
) -> Result<(String, f64)> {
    let output = format!("base{}", next_label_suffix(next_label));
    let pad = (timeline - end).max(0.0);
    if pad > 0.01 {
        filters.push(format!(
            "[{label}]tpad=stop_mode=add:stop_duration={pad:.4}:color=black,trim=duration={timeline:.4},setpts=PTS-STARTPTS,format=yuv420p,settb=AVTB[{output}]"
        ));
    } else {
        filters.push(format!(
            "[{label}]trim=duration={timeline:.4},setpts=PTS-STARTPTS,format=yuv420p,settb=AVTB[{output}]"
        ));
    }
    let _ = (width, height);
    Ok((output, timeline))
}

fn next_label_suffix(counter: &mut usize) -> usize {
    let value = *counter;
    *counter += 1;
    value
}

#[derive(Default)]
struct LabelAllocator {
    counters: HashMap<&'static str, usize>,
}

impl LabelAllocator {
    fn next(&mut self, prefix: &'static str) -> String {
        let counter = self.counters.entry(prefix).or_insert(0);
        let label = format!("{prefix}{counter}");
        *counter += 1;
        label
    }
}

fn even(value: u32) -> u32 {
    if value % 2 == 0 { value } else { value - 1 }
}

fn command_display(command: &Command) -> String {
    let mut parts = Vec::new();
    parts.push(command.get_program().to_string_lossy().into_owned());
    for arg in command.get_args() {
        let text = arg.to_string_lossy();
        if text.contains(' ') || text.contains('\'') {
            parts.push(format!("'{}'", text.replace('\'', "'\\''")));
        } else {
            parts.push(text.into_owned());
        }
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AudioParams, ClipMotion, MotionEasing, MotionKeyframe};

    fn motion_clip() -> Clip {
        Clip {
            id: "clip-motion".into(),
            name: "motion".into(),
            media_id: None,
            start: 0.0,
            in_point: 0.0,
            out_point: 2.0,
            effects: Vec::new(),
            transition: None,
            audio: AudioParams::default(),
            title: None,
            motion: ClipMotion {
                keyframes: vec![
                    MotionKeyframe {
                        time: 0.0,
                        scale: 1.0,
                        pan_x: 0.0,
                        pan_y: 0.0,
                        rotation: 0.0,
                        opacity: 1.0,
                    },
                    MotionKeyframe {
                        time: 2.0,
                        scale: 1.0,
                        pan_x: 0.0,
                        pan_y: 0.0,
                        rotation: 30.0,
                        opacity: 0.25,
                    },
                ],
                easing: MotionEasing::Linear,
                origin: 0.0,
            },
        }
    }

    #[test]
    fn background_is_composited_after_rotation_and_opacity() {
        let clip = motion_clip();
        let mut filters = Vec::new();
        let mut labels = LabelAllocator::default();
        let output = apply_rgba_motion(
            "frame".into(),
            &clip,
            2.0,
            640,
            360,
            Some("#112233"),
            &mut filters,
            &mut labels,
        );

        assert_eq!(output, "motioncomp0");
        assert_eq!(filters.len(), 3);

        let motion = &filters[0];
        let rotate_at = motion.find("rotate=").expect("rotation filter");
        let alpha_at = motion.find("geq=").expect("opacity filter");
        assert!(rotate_at < alpha_at);
        assert!(motion.contains("a='alpha(X,Y)*("));
        assert!(!motion.contains("overlay="));

        assert!(filters[1].contains("color=c=0x112233"));
        assert!(
            filters[2].starts_with("[motionbg0][motion0]overlay="),
            "background must composite the post-motion frame: {}",
            filters[2]
        );
    }
}
