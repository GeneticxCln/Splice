use std::path::Path;
use std::process::Command;

use splice::catalog::make_effect;
use splice::export::{export_project, export_project_with};
use splice::ffmpeg::Toolchain;
use splice::library::import_media;
use splice::model::{
    AudioParams, ClipMotion, EffectKind, Framing, MediaKind, MotionEasing, MotionKeyframe, Project,
    Title, TitleMotion, TitleMotionPreset, Transition, TransitionKind, default_clip_for,
};
use splice::presets::ExportPreset;
use splice::runner::{Cancelled, RenderControl};
use tempfile::TempDir;

fn ffmpeg(args: &[&str]) {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .expect("run ffmpeg");
    assert!(
        output.status.success(),
        "fixture ffmpeg failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn make_fixtures(dir: &Path) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=640x360:rate=30:duration=4",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000:duration=4",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-shortest",
        dir.join("one.mp4").to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "smptebars=size=640x360:rate=30:duration=3",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=660:sample_rate=48000:duration=3",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-shortest",
        dir.join("two.mp4").to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=1280x720:rate=1:duration=1",
        "-frames:v",
        "1",
        dir.join("still.png").to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=220:sample_rate=48000:duration=8",
        "-af",
        "volume=0.2,aformat=channel_layouts=stereo",
        dir.join("music.wav").to_str().unwrap(),
    ]);
}

#[test]
fn simple_feature_export() {
    let dir = TempDir::new().unwrap();
    make_fixtures(dir.path());
    let tools = Toolchain::detect().unwrap();
    let mut project = Project::new("Smoke");

    let one_path = dir.path().join("one.mp4");
    let two_path = dir.path().join("two.mp4");
    let still_path = dir.path().join("still.png");
    let music_path = dir.path().join("music.wav");
    let one = import_media(&mut project, Some(dir.path()), &tools, &one_path).unwrap();
    let two = import_media(&mut project, Some(dir.path()), &tools, &two_path).unwrap();
    let still = import_media(&mut project, Some(dir.path()), &tools, &still_path).unwrap();
    let music = import_media(&mut project, Some(dir.path()), &tools, &music_path).unwrap();
    assert_eq!(one.kind, MediaKind::Video);
    assert_eq!(still.kind, MediaKind::Image);
    assert_eq!(music.kind, MediaKind::Audio);

    let video_track = project.primary_video_track_mut().unwrap();
    let mut clip1 = default_clip_for(&one, 0.0);
    clip1.out_point = 3.0;
    clip1
        .effects
        .push(make_effect(EffectKind::PanZoom).unwrap());
    video_track.clips.push(clip1);
    let mut clip2 = default_clip_for(&two, 0.0);
    clip2.out_point = 3.0;
    clip2.transition = Some(Transition {
        kind: TransitionKind::WipeLeft,
        duration: 0.6,
    });
    video_track.clips.push(clip2);
    let mut image_clip = default_clip_for(&still, 0.0);
    image_clip.out_point = 3.0;
    image_clip
        .effects
        .push(make_effect(EffectKind::Saturation).unwrap());
    image_clip.transition = Some(Transition {
        kind: TransitionKind::CircleOpen,
        duration: 0.5,
    });
    video_track.clips.push(image_clip);
    project.reflow_primary();

    let audio_track = project
        .tracks
        .iter_mut()
        .find(|track| track.kind == splice::model::TrackKind::Audio)
        .unwrap();
    let mut music_clip = default_clip_for(&music, 0.0);
    music_clip.out_point = 8.0;
    music_clip.audio = AudioParams {
        volume: 0.45,
        fade_in: 1.0,
        fade_out: 2.0,
        muted: false,
    };
    audio_track.clips.push(music_clip);

    let mut opening = Title::opening("Splice Smoke");
    opening.background_opacity = 0.4;
    opening.background = "#0B1118".into();
    project.add_title(opening, 0.2, 2.7);
    project.add_title(Title::credits(), 6.0, 2.2);

    let project_path = dir.path().join("smoke.splice");
    splice::project_io::save_project(&project_path, &mut project, Some(dir.path())).unwrap();
    let loaded = splice::project_io::load_project(&project_path).unwrap();

    let output = dir.path().join("out.mp4");
    let report = export_project(
        &loaded.project,
        project_path.parent().unwrap(),
        &tools,
        ExportPreset::P480p,
        &output,
    )
    .unwrap();
    assert!(output.is_file());
    println!("EXPORTED {} duration={}", output.display(), report.duration);

    let probe = tools.probe(&output).unwrap();
    assert_eq!(probe.kind, MediaKind::Video);
    assert_eq!(probe.width, Some(854));
    assert_eq!(probe.height, Some(480));
    assert!(probe.has_audio);
    assert_eq!(probe.sample_rate, Some(48000));
    assert_eq!(probe.channels, Some(2));
    assert!((probe.duration - report.duration).abs() < 0.2);
    println!(
        "PROBE duration={} codec={:?} audio={:?} rate={:?}",
        probe.duration, probe.video_codec, probe.audio_codec, probe.sample_rate
    );

    // Persist the output for manual inspection while iterating.
    let keep = std::env::var("SPLICE_KEEP_OUTPUT").ok();
    if let Some(keep) = keep {
        let _ = std::fs::copy(&output, keep);
    }
}

#[test]
fn custom_title_motion_renders_through_ass() {
    let dir = TempDir::new().unwrap();
    let tools = Toolchain::detect().unwrap();
    let still_path = dir.path().join("title-still.png");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=0x205070:s=640x360:d=1",
        "-frames:v",
        "1",
        still_path.to_str().unwrap(),
    ]);

    let mut project = Project::new("Title motion");
    let still = import_media(&mut project, Some(dir.path()), &tools, &still_path).unwrap();
    let mut clip = default_clip_for(&still, 0.0);
    clip.out_point = 2.0;
    project.primary_video_track_mut().unwrap().clips.push(clip);
    project.reflow_primary();

    let mut title = Title::opening("Typewriter");
    title.text = "MOTION".into();
    title.size = 72.0;
    title.motion = Some(TitleMotion {
        preset: TitleMotionPreset::Typewriter,
        in_duration: 1.0,
        out_duration: 0.3,
        ..TitleMotion::default()
    });
    project.add_title(title, 0.1, 2.4);

    let output = dir.path().join("title-motion.mp4");
    let report =
        export_project(&project, dir.path(), &tools, ExportPreset::P480p, &output).unwrap();
    assert!(report.duration > 2.4);
    assert!(output.is_file());
    let probe = tools.probe(&output).unwrap();
    assert_eq!(probe.video_codec.as_deref(), Some("h264"));
    assert_eq!(probe.width, Some(854));
    assert_eq!(probe.height, Some(480));
}

#[test]
fn keyframed_scale_pan_rotation_and_opacity_render_through_ffmpeg() {
    let dir = TempDir::new().unwrap();
    let tools = Toolchain::detect().unwrap();
    let still_path = dir.path().join("motion-still.png");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=0x205070:s=640x360:d=1",
        "-frames:v",
        "1",
        still_path.to_str().unwrap(),
    ]);

    let mut project = Project::new("Clip motion");
    let still = import_media(&mut project, Some(dir.path()), &tools, &still_path).unwrap();
    let mut clip = default_clip_for(&still, 0.0);
    clip.out_point = 2.0;
    clip.motion = ClipMotion {
        keyframes: vec![
            MotionKeyframe {
                time: 0.0,
                scale: 1.0,
                pan_x: -0.25,
                pan_y: 0.0,
                rotation: -8.0,
                opacity: 1.0,
            },
            MotionKeyframe {
                time: 2.0,
                scale: 1.35,
                pan_x: 0.25,
                pan_y: -0.1,
                rotation: 8.0,
                opacity: 0.55,
            },
        ],
        easing: MotionEasing::EaseInOut,
        origin: 0.0,
    };
    project.primary_video_track_mut().unwrap().clips.push(clip);
    project.reflow_primary();

    let output = dir.path().join("clip-motion.mp4");
    let report =
        export_project(&project, dir.path(), &tools, ExportPreset::P480p, &output).unwrap();
    assert!(report.duration > 1.9);
    assert!(output.is_file());
    let probe = tools.probe(&output).unwrap();
    assert_eq!(probe.video_codec.as_deref(), Some("h264"));
    assert_eq!(probe.width, Some(854));
    assert_eq!(probe.height, Some(480));
}

#[test]
fn shorts_export_is_vertical_and_fills() {
    let dir = TempDir::new().unwrap();
    make_fixtures(dir.path());
    let tools = Toolchain::detect().unwrap();
    let mut project = Project::new_shorts("Short");
    assert!(project.settings.is_short());
    assert_eq!(project.settings.framing, Framing::Fill);

    let one_path = dir.path().join("one.mp4");
    let one = import_media(&mut project, Some(dir.path()), &tools, &one_path).unwrap();
    let mut clip = default_clip_for(&one, 0.0);
    clip.out_point = 1.0;
    project.primary_video_track_mut().unwrap().clips.push(clip);
    project.reflow_primary();

    let output = dir.path().join("short.mp4");
    let report = export_project(
        &project,
        dir.path(),
        &tools,
        ExportPreset::PShorts720p,
        &output,
    )
    .unwrap();
    assert!(report.duration > 0.9);
    let probe = tools.probe(&output).unwrap();
    assert_eq!(probe.width, Some(720));
    assert_eq!(probe.height, Some(1280));
    assert_eq!(probe.video_codec.as_deref(), Some("h264"));
    assert_eq!(probe.audio_codec.as_deref(), Some("aac"));
    assert!(probe.has_audio);
}

#[test]
fn title_file_is_optional_for_untitled_project() {
    // A second tiny guard kept separate so the main smoke test stays focused.
    let dir = TempDir::new().unwrap();
    let tools = Toolchain::detect().unwrap();
    let mut project = Project::new("No titles");
    let still_path = dir.path().join("still.png");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=0x205070:s=640x360:d=1",
        "-frames:v",
        "1",
        still_path.to_str().unwrap(),
    ]);
    let still = import_media(&mut project, Some(dir.path()), &tools, &still_path).unwrap();
    let clip = default_clip_for(&still, 0.0);
    project.primary_video_track_mut().unwrap().clips.push(clip);
    let loaded = project.clone();
    let output = dir.path().join("still.mp4");
    let report = export_project(&loaded, dir.path(), &tools, ExportPreset::P480p, &output).unwrap();
    assert!(report.duration > 0.0);
    println!("STILL_EXPORT_OK {}", report.duration);
}

/// A single clip with hundreds of eased keyframes. Its generated filter graph
/// is larger than the 128 KiB Linux per-argument limit, so it can only run if
/// the graph is handed to ffmpeg by file.
fn keyframe_heavy_project(dir: &Path, tools: &Toolchain, keyframes: usize) -> Project {
    let still_path = dir.join("heavy-still.png");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=0x305060:s=640x360:d=1",
        "-frames:v",
        "1",
        still_path.to_str().unwrap(),
    ]);
    let mut project = Project::new("Heavy motion");
    let still = import_media(&mut project, Some(dir), tools, &still_path).unwrap();
    let mut clip = default_clip_for(&still, 0.0);
    clip.out_point = keyframes as f64 * 0.04;
    clip.motion = ClipMotion {
        keyframes: (0..keyframes)
            .map(|index| MotionKeyframe {
                time: index as f64 * 0.04,
                scale: 1.0 + (index % 7) as f64 * 0.05,
                pan_x: ((index % 5) as f64 - 2.0) * 0.1,
                pan_y: ((index % 3) as f64 - 1.0) * 0.1,
                rotation: 0.0,
                opacity: 1.0,
            })
            .collect(),
        easing: MotionEasing::EaseInOut,
        origin: 0.0,
    };
    project.primary_video_track_mut().unwrap().clips.push(clip);
    project.reflow_primary();
    project
}

#[test]
fn filter_graph_larger_than_the_argv_limit_still_renders() {
    let dir = TempDir::new().unwrap();
    let tools = Toolchain::detect().unwrap();
    let project = keyframe_heavy_project(dir.path(), &tools, 400);
    let output = dir.path().join("heavy.mp4");
    let control = RenderControl::new();
    let report = export_project_with(
        &project,
        dir.path(),
        &tools,
        ExportPreset::P480p,
        &output,
        &control,
    )
    .expect("export with a graph over 128 KiB");
    assert!(
        report.graph_bytes > 131_072,
        "fixture graph is only {} bytes; it must exceed the argv limit",
        report.graph_bytes
    );
    assert!(output.is_file());
    assert!(
        control.progress_seconds() > report.duration * 0.9,
        "progress {} of {}",
        control.progress_seconds(),
        report.duration
    );
}

#[test]
fn export_cancel_stops_ffmpeg_and_leaves_no_files() {
    let dir = TempDir::new().unwrap();
    let tools = Toolchain::detect().unwrap();
    let mut project = keyframe_heavy_project(dir.path(), &tools, 500);
    // Slow filters keep the 1080p encode running for seconds, so the cancel
    // always lands mid-render rather than after it finished.
    let clip = &mut project.primary_video_track_mut().unwrap().clips[0];
    for kind in [EffectKind::Bilateral, EffectKind::Median, EffectKind::Blur] {
        clip.effects.push(make_effect(kind).unwrap());
    }
    let out_dir = dir.path().join("out");
    let output = out_dir.join("cancelled.mp4");
    let control = RenderControl::new();

    // Cancel as soon as ffmpeg reports its first encoded frame.
    let watcher = control.clone();
    let canceller = std::thread::spawn(move || {
        let started = std::time::Instant::now();
        while watcher.progress_seconds() <= 0.0 && started.elapsed().as_secs() < 60 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        watcher.cancel();
    });
    let error = export_project_with(
        &project,
        dir.path(),
        &tools,
        ExportPreset::P1080p,
        &output,
        &control,
    )
    .expect_err("export must be cancelled");
    canceller.join().unwrap();

    assert!(error.downcast_ref::<Cancelled>().is_some(), "{error:#}");
    assert!(!output.exists(), "cancelled export left its output");
    let leftovers: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );
}
