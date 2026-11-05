use std::path::Path;
use std::process::Command;

use splice::catalog::make_effect;
use splice::export::export_project;
use splice::ffmpeg::Toolchain;
use splice::library::import_media;
use splice::model::{
    AudioParams, EffectKind, Framing, MediaKind, Project, Title, Transition, TransitionKind,
    default_clip_for,
};
use splice::presets::ExportPreset;
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
