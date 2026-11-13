//! Scale checks on a ten minute, 120 clip project. They take a minute or two
//! of real encoding, so they are ignored by default:
//!
//!     cargo test --release --test scale -- --ignored --nocapture
//!
//! The budgets are deliberately generous; they exist to catch the kind of
//! regression where cost grows with source length or clip count (decoding
//! every file from its start, re-rendering the whole timeline for a preview).

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use splice::export::export_project;
use splice::ffmpeg::Toolchain;
use splice::library::import_media;
use splice::model::{Project, default_clip_for};
use splice::presets::ExportPreset;
use splice::render::{RenderOptions, render_project};
use splice::window::{choose_window, project_window};
use tempfile::TempDir;

fn long_project(dir: &Path, tools: &Toolchain) -> Project {
    // A two minute source so late in-points matter: if each clip decoded its
    // file from the start, clip 120 would pay for ~110 s of video.
    let source = dir.join("source.mp4");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=30:duration=120",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=330:sample_rate=48000:duration=120",
        ])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "60"])
        .args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(&source)
        .status()
        .expect("ffmpeg");
    assert!(status.success());

    let mut project = Project::new("Ten minutes");
    let asset = import_media(&mut project, Some(dir), tools, &source).expect("import");
    for index in 0..120 {
        let mut clip = default_clip_for(&asset, 0.0);
        clip.in_point = (index % 23) as f64 * 5.0;
        clip.out_point = clip.in_point + 5.0;
        project.primary_video_track_mut().unwrap().clips.push(clip);
    }
    project.reflow_primary();
    assert!((project.timeline_duration() - 600.0).abs() < 0.01);
    project
}

#[test]
#[ignore = "encodes ten minutes of video; run explicitly"]
fn ten_minute_project_previews_and_exports_within_budget() {
    let dir = TempDir::new().unwrap();
    let tools = Toolchain::detect().unwrap();
    let project = long_project(dir.path(), &tools);

    // One preview slice from the middle of the timeline.
    let (start, end) = choose_window(300.0, project.timeline_duration()).expect("windowed");
    let started = Instant::now();
    let sliced = project_window(&project, start, end);
    let output = dir.path().join("slice.mp4");
    let options = RenderOptions {
        width: 960,
        height: 540,
        preview: true,
        allow_offline: false,
        level: "3.1".into(),
        output: output.clone(),
        control: Default::default(),
        min_duration: end - start,
    };
    let report = render_project(&sliced, dir.path(), &tools, &options).expect("slice");
    let slice_time = started.elapsed();
    println!(
        "preview slice {start:.0}-{end:.0}s: {slice_time:.1?} ({} byte graph)",
        report.graph_bytes
    );
    assert!(
        slice_time.as_secs() < 30,
        "a 30 s preview slice took {slice_time:?}"
    );

    // The whole movie.
    let started = Instant::now();
    let movie = dir.path().join("movie.mp4");
    let exported =
        export_project(&project, dir.path(), &tools, ExportPreset::P480p, &movie).expect("export");
    let export_time = started.elapsed();
    println!(
        "480p export of {:.0}s, 120 clips: {export_time:.1?} ({} byte graph)",
        exported.duration, exported.graph_bytes
    );
    assert!(movie.is_file());
    assert!(
        export_time.as_secs() < 600,
        "480p export took {export_time:?}"
    );
}
