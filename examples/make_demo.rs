//! Builds the reproducible demo project from locally generated media.
//!
//! Usage: `cargo run --release --example make_demo -- <media-dir> <output.splice>`
//!
//! Every asset goes through `library::import_media`, so the demo is probed by
//! the real system ffprobe and stored with relative paths next to the project.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use splice::catalog::make_effect;
use splice::edit;
use splice::ffmpeg::Toolchain;
use splice::library::import_media;
use splice::model::{
    default_clip_for, AudioParams, EffectKind, Project, Title, Transition, TransitionKind,
};
use splice::project_io::save_project;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let media_dir = PathBuf::from(args.next().unwrap_or_else(|| "docs/media".into()));
    let output = PathBuf::from(args.next().unwrap_or_else(|| "docs/demo.splice".into()));
    let project_dir = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    std::fs::create_dir_all(&project_dir)
        .with_context(|| format!("could not create {}", project_dir.display()))?;

    let tools = Toolchain::detect().context("system ffmpeg and ffprobe are required")?;

    let required = [
        "clip-a.mp4",
        "clip-b.mp4",
        "clip-c.mp4",
        "photo.png",
        "music.m4a",
    ];
    for name in required {
        let path = media_dir.join(name);
        if !path.is_file() {
            bail!(
                "missing demo media {}; run scripts/demo.sh to generate the fixtures",
                path.display()
            );
        }
    }

    let mut project = Project::new("Splice Demo Reel");
    let clip_a = import_media(
        &mut project,
        Some(&project_dir),
        &tools,
        &media_dir.join("clip-a.mp4"),
    )?;
    let clip_b = import_media(
        &mut project,
        Some(&project_dir),
        &tools,
        &media_dir.join("clip-b.mp4"),
    )?;
    let clip_c = import_media(
        &mut project,
        Some(&project_dir),
        &tools,
        &media_dir.join("clip-c.mp4"),
    )?;
    let photo = import_media(
        &mut project,
        Some(&project_dir),
        &tools,
        &media_dir.join("photo.png"),
    )?;
    let music = import_media(
        &mut project,
        Some(&project_dir),
        &tools,
        &media_dir.join("music.m4a"),
    )?;

    // Opening clip: trimmed to the full source, then split in half to exercise
    // the trim/split path while keeping both halves in the storyboard.
    let mut first = default_clip_for(&clip_a, 0.0);
    first.out_point = 6.0;
    first.audio = AudioParams {
        volume: 0.7,
        fade_in: 0.3,
        ..AudioParams::default()
    };
    for kind in [
        EffectKind::Brightness,
        EffectKind::Contrast,
        EffectKind::Saturation,
    ] {
        first
            .effects
            .push(make_effect(kind).context("catalog effect")?);
    }
    let first_id = first.id.clone();
    project
        .primary_video_track_mut()
        .expect("primary video track")
        .clips
        .push(first);
    edit::reflow_primary_ordered(&mut project);

    let second_id = edit::split_clip(&mut project, &first_id, 3.0)?
        .context("splitting the opening clip must create a right half")?;
    {
        let (_, second) = project
            .clip_by_id_mut(&second_id)
            .context("split clip must exist")?;
        second.transition = Some(Transition {
            kind: TransitionKind::Dissolve,
            duration: 0.5,
        });
        second.audio.muted = true;
        second
            .effects
            .push(make_effect(EffectKind::Temperature).context("temperature effect")?);
    }

    // Mandelbrot clip with a fade and a vignette/sharpen combo.
    let mut second = default_clip_for(&clip_b, 0.0);
    second.out_point = 5.5;
    second.transition = Some(Transition {
        kind: TransitionKind::Fade,
        duration: 0.6,
    });
    second.audio = AudioParams {
        volume: 0.5,
        ..AudioParams::default()
    };
    second
        .effects
        .push(make_effect(EffectKind::Vignette).context("vignette effect")?);
    second
        .effects
        .push(make_effect(EffectKind::Sharpen).context("sharpen effect")?);
    project
        .primary_video_track_mut()
        .expect("primary video track")
        .clips
        .push(second);

    // Still image with animated pan-and-zoom and a wipe.
    let mut still = default_clip_for(&photo, 0.0);
    still.out_point = 4.0;
    still.transition = Some(Transition {
        kind: TransitionKind::WipeLeft,
        duration: 0.5,
    });
    still
        .effects
        .push(make_effect(EffectKind::PanZoom).context("pan-zoom effect")?);
    still
        .effects
        .push(make_effect(EffectKind::Blur).context("blur effect")?);
    project
        .primary_video_track_mut()
        .expect("primary video track")
        .clips
        .push(still);

    // Closing colour bars with a circular reveal and hue rotation.
    let mut last = default_clip_for(&clip_c, 0.0);
    last.out_point = 5.5;
    last.transition = Some(Transition {
        kind: TransitionKind::CircleOpen,
        duration: 0.5,
    });
    last.effects
        .push(make_effect(EffectKind::HueRotate).context("hue effect")?);
    project
        .primary_video_track_mut()
        .expect("primary video track")
        .clips
        .push(last);

    edit::reflow_primary_ordered(&mut project);

    let mut opening = Title::opening("Splice Demo Reel");
    opening.subtitle = "Rust + egui · non-destructive editing".to_string();
    opening.background = "#0B1118".to_string();
    opening.background_opacity = 0.35;
    project.add_title(opening, 0.3, 3.4);

    let mut credits = Title::credits();
    credits.text = "THANKS FOR WATCHING".to_string();
    credits.subtitle = "Cut, scored and rendered with Splice".to_string();
    project.add_title(credits, project.primary_end(), 2.6);

    let duration = project.timeline_duration();
    let mut score = default_clip_for(&music, 0.0);
    score.out_point = duration;
    score.audio = AudioParams {
        volume: 0.55,
        fade_in: 1.0,
        fade_out: 2.0,
        muted: false,
    };
    project
        .first_audio_track_mut()
        .context("project has no music track")?
        .clips
        .push(score);

    save_project(&output, &mut project, Some(&project_dir))?;

    println!(
        "DEMO_PROJECT_OK {} duration={:.2}s media={} video_clips={} title_clips={}",
        output.display(),
        project.timeline_duration(),
        project.media.len(),
        project
            .primary_video_track()
            .map(|track| track.clips.len())
            .unwrap_or(0),
        project.title_clips().len()
    );
    Ok(())
}
