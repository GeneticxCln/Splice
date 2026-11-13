//! End-to-end acceptance suite for Splice.
//!
//! These tests use the real system ffmpeg/ffprobe and locally generated media.
//! No network is used. Tolerances are documented next to the assertions:
//!   * container duration: ±0.35 s (AAC priming and 1-frame rounding)
//!   * frame-difference means: > 2/255 for a visible title or camera move
//!   * monochrome saturation: < 4/255 (8-bit round-trip noise)
//!   * audio fade windows: at least 6 dB below the sustained middle level

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};
use splice::automovie::{self, AutoMovieOptions};
use splice::catalog::{EFFECT_CATEGORIES, EFFECTS, TITLE_STYLES, TRANSITIONS, make_effect};
use splice::edit;
use splice::export::export_project;
use splice::ffmpeg::Toolchain;
use splice::library::{import_media, relink_media};
use splice::model::{
    AudioParams, EffectKind, Framing, MediaAsset, MediaKind, Project, Title, TitleStyle,
    Transition, TransitionKind, default_clip_for,
};
use splice::presets::ExportPreset;
use splice::project_io::{load_project, save_project};
use splice::render::{RenderOptions, render_project};
use tempfile::TempDir;

struct Fixtures {
    dir: TempDir,
    one: PathBuf,
    two: PathBuf,
    still: PathBuf,
    music: PathBuf,
}

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

fn fixtures() -> Fixtures {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path();
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
        path.join("one.mp4").to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "smptebars=size=640x360:rate=30:duration=4",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=660:sample_rate=48000:duration=4",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-shortest",
        path.join("two.mp4").to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=640x360:rate=1:duration=1",
        "-frames:v",
        "1",
        path.join("still.png").to_str().unwrap(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=220:sample_rate=48000:duration=14",
        "-af",
        "volume=0.35,aformat=channel_layouts=stereo",
        path.join("music.wav").to_str().unwrap(),
    ]);
    Fixtures {
        one: path.join("one.mp4"),
        two: path.join("two.mp4"),
        still: path.join("still.png"),
        music: path.join("music.wav"),
        dir,
    }
}

fn empty_project() -> Project {
    Project::new("Acceptance")
}

fn import_all(
    project: &mut Project,
    f: &Fixtures,
    tools: &Toolchain,
) -> (MediaAsset, MediaAsset, MediaAsset, MediaAsset) {
    let project_dir = Some(f.dir.path());
    let one = import_media(project, project_dir, tools, &f.one).expect("import one");
    let two = import_media(project, project_dir, tools, &f.two).expect("import two");
    let still = import_media(project, project_dir, tools, &f.still).expect("import still");
    let music = import_media(project, project_dir, tools, &f.music).expect("import music");
    (one, two, still, music)
}

fn sha256(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("read source for hash");
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn source_hashes(f: &Fixtures) -> Vec<(PathBuf, String)> {
    [&f.one, &f.two, &f.still, &f.music]
        .into_iter()
        .map(|path| (path.clone(), sha256(path)))
        .collect()
}

fn rich_project(f: &Fixtures, tools: &Toolchain) -> (Project, PathBuf) {
    let mut project = empty_project();
    let (one, two, still, music) = import_all(&mut project, f, tools);

    let mut first = default_clip_for(&one, 0.0);
    first.out_point = 2.4;
    first.audio = AudioParams {
        volume: 0.55,
        fade_in: 0.4,
        fade_out: 0.2,
        muted: false,
    };
    for kind in [
        EffectKind::Brightness,
        EffectKind::Contrast,
        EffectKind::Saturation,
        EffectKind::HueRotate,
        EffectKind::Sharpen,
    ] {
        first.effects.push(make_effect(kind).expect("effect"));
    }
    let first_id = first.id.clone();
    project.primary_video_track_mut().unwrap().clips.push(first);

    let mut second = default_clip_for(&two, 0.0);
    second.out_point = 2.6;
    second
        .effects
        .push(make_effect(EffectKind::Vignette).unwrap());
    second
        .effects
        .push(make_effect(EffectKind::Temperature).unwrap());
    second.effects.push({
        let mut effect = make_effect(EffectKind::Monochrome).unwrap();
        effect.params.insert("amount".into(), 0.2);
        effect
    });
    second.transition = Some(Transition {
        kind: TransitionKind::Fade,
        duration: 0.6,
    });
    project
        .primary_video_track_mut()
        .unwrap()
        .clips
        .push(second);

    let mut photo = default_clip_for(&still, 0.0);
    photo.out_point = 2.0;
    photo
        .effects
        .push(make_effect(EffectKind::PanZoom).unwrap());
    photo.effects.push(make_effect(EffectKind::Blur).unwrap());
    photo.transition = Some(Transition {
        kind: TransitionKind::WipeLeft,
        duration: 0.5,
    });
    let photo_id = photo.id.clone();
    project.primary_video_track_mut().unwrap().clips.push(photo);

    edit::reflow_primary_ordered(&mut project);

    // Trim + split: the 2.4 s opening clip becomes 1.2 s + 1.2 s.
    let right_id = edit::split_clip(&mut project, &first_id, 1.2)
        .expect("split inside clip")
        .expect("split creates a clip");
    assert!(project.clip_by_id(&right_id).is_some());
    let left = project.clip_by_id(&first_id).unwrap().1.duration();
    let right = project.clip_by_id(&right_id).unwrap().1.duration();
    assert!((left - 1.2).abs() < 0.001 && (right - 1.2).abs() < 0.001);

    // Duplicate then delete the copy, proving both edits preserve the timeline.
    let before = project.primary_video_track().unwrap().clips.len();
    let duplicate = edit::duplicate_clip(&mut project, &right_id)
        .expect("duplicate")
        .expect("duplicate id");
    assert_eq!(
        project.primary_video_track().unwrap().clips.len(),
        before + 1
    );
    assert!(edit::delete_clip(&mut project, &duplicate));
    assert_eq!(project.primary_video_track().unwrap().clips.len(), before);

    // Reorder the still left then right again.
    assert!(edit::move_primary_clip_by(&mut project, &photo_id, -1));
    assert!(edit::move_primary_clip_by(&mut project, &photo_id, 1));
    assert_eq!(
        project
            .primary_video_track()
            .unwrap()
            .clips
            .last()
            .map(|clip| clip.id.as_str()),
        Some(photo_id.as_str())
    );

    let mut opening = Title::opening("Acceptance Reel");
    opening.background_opacity = 0.35;
    opening.background = "#0B1118".into();
    project.add_title(opening, 0.2, 2.7);
    let mut credits = Title::credits();
    credits.text = "THE END".to_string();
    credits.subtitle = "Rendered by the acceptance suite".to_string();
    project.add_title(credits, project.primary_end(), 2.2);

    let mut music_clip = default_clip_for(&music, 0.0);
    music_clip.out_point = 8.0;
    music_clip.audio = AudioParams {
        volume: 0.5,
        fade_in: 1.0,
        fade_out: 1.5,
        muted: false,
    };
    project
        .first_audio_track_mut()
        .unwrap()
        .clips
        .push(music_clip);

    let project_path = f.dir.path().join("acceptance.splice");
    (project, project_path)
}

#[test]
fn catalog_and_presets_are_complete() {
    assert_eq!(TRANSITIONS.len(), TransitionKind::ALL.len());
    assert_eq!(EFFECTS.len(), EffectKind::ALL.len());
    assert_eq!(TITLE_STYLES.len(), 10);
    assert!(
        TRANSITIONS.len() >= 58,
        "the full ffmpeg xfade set must be exposed"
    );
    assert!(EFFECTS.len() >= 30, "the effect catalog must be expanded");

    for kind in TransitionKind::ALL {
        assert!(
            TRANSITIONS.iter().any(|(_, candidate)| candidate == kind),
            "transition {kind:?} is missing from the catalog"
        );
    }
    let transitions: HashSet<&str> = TransitionKind::ALL
        .iter()
        .map(|kind| kind.xfade_name())
        .collect();
    assert_eq!(
        transitions.len(),
        TransitionKind::ALL.len(),
        "transition xfade names must be unique"
    );

    for kind in EffectKind::ALL {
        assert!(
            EFFECTS.iter().any(|definition| definition.kind == *kind),
            "effect {kind:?} is missing from the catalog"
        );
    }
    let effect_kinds: HashSet<String> = EFFECTS
        .iter()
        .map(|definition| format!("{:?}", definition.kind))
        .collect();
    assert_eq!(
        effect_kinds.len(),
        EffectKind::ALL.len(),
        "effect kinds must be unique"
    );
    let mut categorized = Vec::new();
    for (_, kinds) in EFFECT_CATEGORIES {
        categorized.extend_from_slice(kinds);
    }
    assert_eq!(
        categorized.len(),
        EffectKind::ALL.len(),
        "every effect must belong to exactly one picker category"
    );
    for kind in EffectKind::ALL {
        assert_eq!(
            categorized
                .iter()
                .filter(|candidate| *candidate == kind)
                .count(),
            1,
            "effect {kind:?} must appear in exactly one category"
        );
    }

    for definition in EFFECTS {
        let effect = make_effect(definition.kind).expect("make effect");
        assert_eq!(
            effect.params.len(),
            definition.params.len(),
            "{} must materialize every parameter",
            definition.name
        );
        let mut keys = HashSet::new();
        for param in definition.params {
            assert!(
                keys.insert(param.key),
                "{} has duplicate parameter {}",
                definition.name,
                param.key
            );
        }
    }

    let styles: HashSet<String> = TITLE_STYLES
        .iter()
        .map(|(_, style)| format!("{style:?}"))
        .collect();
    assert_eq!(styles.len(), 10, "title styles must be unique");
    let _ = TitleStyle::Fade;

    let presets = [
        (ExportPreset::P1080p, 1920, 1080, "4.1"),
        (ExportPreset::P720p, 1280, 720, "3.2"),
        (ExportPreset::P480p, 854, 480, "3.1"),
        (ExportPreset::PShorts1080p, 1080, 1920, "4.1"),
        (ExportPreset::PShorts720p, 720, 1280, "3.2"),
    ];
    for (preset, width, height, level) in presets {
        assert_eq!(preset.width(), width);
        assert_eq!(preset.height(), height);
        assert_eq!(preset.level(), level);
    }
}

#[test]
fn projects_without_framing_default_to_fit() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("old.splice");
    let document = serde_json::json!({
        "format": "splice-project",
        "version": 1,
        "name": "Old project",
        "settings": {
            "width": 1920,
            "height": 1080,
            "fps": 30,
            "sample_rate": 48000
        },
        "media": [],
        "tracks": []
    });
    std::fs::write(&path, serde_json::to_string_pretty(&document).unwrap()).unwrap();
    let loaded = load_project(&path).expect("old project should load");
    assert_eq!(loaded.project.settings.framing, Framing::Fit);
}

#[test]
fn project_roundtrip_is_versioned_and_rejects_newer_files() {
    let f = fixtures();
    let tools = Toolchain::detect().expect("ffmpeg/ffprobe");
    let (mut project, path) = rich_project(&f, &tools);
    let before = project.clone();

    save_project(&path, &mut project, Some(f.dir.path())).expect("save project");
    let text = std::fs::read_to_string(&path).expect("read project");
    let envelope: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(envelope["format"], "splice-project");
    assert_eq!(envelope["version"], 1);
    assert!(
        envelope["media"][0]["path"]
            .as_str()
            .unwrap()
            .starts_with("one.mp4")
    );

    let loaded = load_project(&path).expect("load project");
    assert_eq!(loaded.project.media, before.media);
    assert_eq!(loaded.project.tracks.len(), before.tracks.len());
    assert!(
        (loaded.project.timeline_duration() - before.timeline_duration()).abs() < 1e-6,
        "timeline duration must round-trip"
    );
    // A second save/load cycle must be bit-stable. The first cycle may move a
    // floating point start by one ULP while serde writes the shortest decimal.
    let mut loaded = loaded;
    save_project(&path, &mut loaded.project, Some(f.dir.path())).expect("save again");
    let reloaded = load_project(&path).expect("reload project");
    assert_eq!(reloaded.project, loaded.project);

    // A newer schema must be refused, not guessed.
    let mut newer = envelope.clone();
    newer["version"] = serde_json::json!(2);
    let newer_path = f.dir.path().join("newer.splice");
    std::fs::write(&newer_path, serde_json::to_string_pretty(&newer).unwrap()).unwrap();
    let error = load_project(&newer_path).expect_err("version 2 must be rejected");
    assert!(
        format!("{error:#}").contains("unsupported project version 2"),
        "unexpected error: {error:#}"
    );

    // Extensions are part of the contract.
    let wrong_extension = f.dir.path().join("project.json");
    std::fs::copy(&path, &wrong_extension).unwrap();
    let error = load_project(&wrong_extension).expect_err("non-.splice must be rejected");
    assert!(format!("{error:#}").contains(".splice extension"));
}

#[test]
fn full_feature_export_matches_every_preset() {
    let f = fixtures();
    let tools = Toolchain::detect().expect("ffmpeg/ffprobe");
    let hashes = source_hashes(&f);
    let (mut project, project_path) = rich_project(&f, &tools);
    save_project(&project_path, &mut project, Some(f.dir.path())).expect("save");
    let loaded = load_project(&project_path).expect("load");

    for preset in ExportPreset::ALL {
        let output = f
            .dir
            .path()
            .join(format!("acceptance-{}.mp4", preset.label()));
        let report = export_project(
            &loaded.project,
            project_path.parent().unwrap(),
            &tools,
            preset,
            &output,
        )
        .expect("export");
        assert!(output.is_file());
        assert!(report.duration > 4.0);

        let probe = tools.probe(&output).expect("probe export");
        assert_eq!(probe.kind, MediaKind::Video);
        assert_eq!(probe.video_codec.as_deref(), Some("h264"));
        assert_eq!(probe.audio_codec.as_deref(), Some("aac"));
        assert_eq!(probe.width, Some(preset.width()));
        assert_eq!(probe.height, Some(preset.height()));
        assert_eq!(probe.sample_rate, Some(48_000));
        assert_eq!(probe.channels, Some(2));
        assert!(probe.has_audio);
        assert!(
            (probe.duration - report.duration).abs() < 0.35,
            "preset {} duration {} vs report {}",
            preset.label(),
            probe.duration,
            report.duration
        );

        let fps = ffprobe_field(&output, "stream=r_frame_rate");
        assert_eq!(
            fps.as_deref(),
            Some("30/1"),
            "30 fps for {}",
            preset.label()
        );
    }

    for (path, expected) in hashes {
        assert_eq!(
            sha256(&path),
            expected,
            "source was modified: {}",
            path.display()
        );
    }
}

#[test]
fn feature_matrix_renders_all_transitions_effects_and_titles() {
    let f = fixtures();
    let tools = Toolchain::detect().expect("ffmpeg/ffprobe");

    // Every transition, one per short clip.
    let mut transitions_project = empty_project();
    let (one, _, _, _) = import_all(&mut transitions_project, &f, &tools);
    for (index, (_, transition)) in TRANSITIONS.iter().enumerate() {
        let mut clip = default_clip_for(&one, 0.0);
        clip.name = format!("Transition {}", index + 1);
        clip.out_point = 0.45;
        if index > 0 {
            clip.transition = Some(Transition {
                kind: *transition,
                duration: 0.12,
            });
        }
        transitions_project
            .primary_video_track_mut()
            .unwrap()
            .clips
            .push(clip);
    }
    edit::reflow_primary_ordered(&mut transitions_project);

    // Every animated title style, spread across the transition sequence.
    for (index, (_, style)) in TITLE_STYLES.iter().enumerate() {
        let mut title = Title::opening("Style matrix");
        title.text = format!("STYLE {}", index + 1);
        title.style = *style;
        title.size = 54.0;
        transitions_project.add_title(title, 0.1 + index as f64 * 0.6, 0.5);
    }

    let transition_output = f.dir.path().join("matrix-transitions.mp4");
    let options = RenderOptions {
        width: 480,
        height: 270,
        preview: true,
        allow_offline: false,
        level: "3.1".into(),
        output: transition_output.clone(),
        control: Default::default(),
    };
    let transition_report = render_project(&transitions_project, f.dir.path(), &tools, &options)
        .expect("transition matrix render");
    assert!(transition_report.duration > 10.0);
    let probe = tools
        .probe(&transition_output)
        .expect("probe transition matrix");
    assert_eq!(probe.video_codec.as_deref(), Some("h264"));
    assert_eq!(probe.width, Some(480));
    assert_eq!(probe.height, Some(270));

    // Every effect, one per short clip.
    let mut effects_project = empty_project();
    let effect_asset = import_media(&mut effects_project, Some(f.dir.path()), &tools, &f.one)
        .expect("import effect source");
    for (index, definition) in EFFECTS.iter().enumerate() {
        let mut clip = default_clip_for(&effect_asset, 0.0);
        clip.name = format!("Effect {} {}", index + 1, definition.name);
        clip.out_point = 0.3;
        clip.effects.push(make_effect(definition.kind).unwrap());
        effects_project
            .primary_video_track_mut()
            .unwrap()
            .clips
            .push(clip);
    }
    edit::reflow_primary_ordered(&mut effects_project);

    let effects_output = f.dir.path().join("matrix-effects.mp4");
    let options = RenderOptions {
        width: 480,
        height: 270,
        preview: true,
        allow_offline: false,
        level: "3.1".into(),
        output: effects_output.clone(),
        control: Default::default(),
    };
    let effects_report = render_project(&effects_project, f.dir.path(), &tools, &options)
        .expect("effect matrix render");
    assert!(effects_report.duration > 8.0);
    let probe = tools.probe(&effects_output).expect("probe effect matrix");
    assert_eq!(probe.video_codec.as_deref(), Some("h264"));
    assert_eq!(probe.width, Some(480));
    assert_eq!(probe.height, Some(270));
    assert!(effects_output.is_file());
}

#[test]
fn automovie_builds_titles_transitions_and_music() {
    let f = fixtures();
    let tools = Toolchain::detect().expect("ffmpeg/ffprobe");
    let mut project = empty_project();
    let (one, two, still, music) = import_all(&mut project, &f, &tools);
    for (index, asset) in [&one, &two, &still].into_iter().enumerate() {
        let mut clip = default_clip_for(asset, 0.0);
        if asset.kind == MediaKind::Video {
            clip.out_point = 2.5;
        } else {
            clip.out_point = 2.0;
        }
        clip.start = index as f64 * 2.5;
        project.primary_video_track_mut().unwrap().clips.push(clip);
    }
    edit::reflow_primary_ordered(&mut project);

    let options = AutoMovieOptions {
        music_media_id: Some(music.id.clone()),
        fit_to_music: true,
        ..AutoMovieOptions::default()
    };
    let report = automovie::build(&mut project, &options).expect("automovie");
    assert_eq!(report.clips, 3);
    assert!(report.music.is_some());
    assert!(
        (report.primary_duration - music.duration).abs() < 0.35,
        "primary {} should fit music {}",
        report.primary_duration,
        music.duration
    );
    assert!(
        project
            .primary_video_track()
            .unwrap()
            .clips
            .iter()
            .skip(1)
            .all(|clip| clip.transition.is_some())
    );
    assert_eq!(project.title_clips().len(), 2);
    let music_track = project
        .tracks
        .iter()
        .find(|track| track.id == "track-audio-music")
        .unwrap();
    assert_eq!(music_track.clips.len(), 1);
    assert_eq!(
        music_track.clips[0].media_id.as_deref(),
        Some(music.id.as_str())
    );

    // Running AutoMovie again must replace, not stack, generated titles.
    automovie::build(&mut project, &options).expect("second automovie");
    assert_eq!(project.title_clips().len(), 2);

    let project_path = f.dir.path().join("automovie.splice");
    save_project(&project_path, &mut project, Some(f.dir.path())).expect("save automovie");
    let output = f.dir.path().join("automovie.mp4");
    let exported = export_project(&project, f.dir.path(), &tools, ExportPreset::P480p, &output)
        .expect("export automovie");
    let probe = tools.probe(&output).expect("probe automovie");
    assert!(probe.has_audio);
    assert!((probe.duration - exported.duration).abs() < 0.35);
    assert!(exported.duration > report.primary_duration);
}

#[test]
fn title_pan_zoom_monochrome_and_audio_fades_are_measured() {
    let f = fixtures();
    let tools = Toolchain::detect().expect("ffmpeg/ffprobe");
    let mut project = empty_project();
    let (_, _, still, music) = import_all(&mut project, &f, &tools);

    // Opening still with a title over a static background, so a frame diff can
    // only come from the title.
    let mut base = default_clip_for(&still, 0.0);
    base.out_point = 2.4;
    project.primary_video_track_mut().unwrap().clips.push(base);
    let mut title = Title::opening("Sample Check");
    title.text = "TITLE CHECK".into();
    title.subtitle = String::new();
    title.size = 72.0;
    title.background_opacity = 0.0;
    project.add_title(title, 0.6, 1.4);

    // Pan & zoom on the same still.
    let mut zoom = default_clip_for(&still, 0.0);
    zoom.out_point = 2.2;
    zoom.effects.push(make_effect(EffectKind::PanZoom).unwrap());
    project.primary_video_track_mut().unwrap().clips.push(zoom);

    // Monochrome version of the same still.
    let mut mono = default_clip_for(&still, 0.0);
    mono.out_point = 2.2;
    mono.effects
        .push(make_effect(EffectKind::Monochrome).unwrap());
    project.primary_video_track_mut().unwrap().clips.push(mono);
    edit::reflow_primary_ordered(&mut project);

    // Music carries the fade, isolated from source-clip audio.
    let mut music_clip = default_clip_for(&music, 0.0);
    music_clip.out_point = project.primary_end();
    music_clip.audio = AudioParams {
        volume: 0.8,
        fade_in: 1.0,
        fade_out: 1.0,
        muted: false,
    };
    project
        .first_audio_track_mut()
        .unwrap()
        .clips
        .push(music_clip);

    let output = f.dir.path().join("looks.mp4");
    let report = export_project(&project, f.dir.path(), &tools, ExportPreset::P480p, &output)
        .expect("export look test");

    // Base still is 2.4 s, zoom 2.2 s, mono 2.2 s (no transitions here).
    let title_off = frame(&tools, &output, 0.15);
    let title_on = frame(&tools, &output, 1.2);
    let title_diff = mean_abs_diff(&title_off, &title_on);
    assert!(
        title_diff > 2.0,
        "title frame difference {title_diff:.2} is below the 2/255 tolerance"
    );

    let zoom_start = frame(&tools, &output, 2.5);
    let zoom_end = frame(&tools, &output, 4.4);
    let zoom_diff = mean_abs_diff(&zoom_start, &zoom_end);
    assert!(
        zoom_diff > 2.0,
        "pan/zoom frame difference {zoom_diff:.2} is below the 2/255 tolerance"
    );

    let mono_frame = frame(&tools, &output, 5.9);
    let saturation = mean_saturation(&mono_frame);
    assert!(
        saturation < 4.0,
        "monochrome saturation {saturation:.2} exceeds the 4/255 tolerance"
    );

    let fade_in = mean_volume(&tools, &output, 0.0, 0.25);
    let middle = mean_volume(&tools, &output, report.duration * 0.45, 0.2);
    let fade_out = mean_volume(&tools, &output, report.duration - 0.3, 0.25);
    assert!(
        middle - fade_in > 6.0,
        "fade-in {fade_in:.1} dB is not 6 dB below middle {middle:.1} dB"
    );
    assert!(
        middle - fade_out > 6.0,
        "fade-out {fade_out:.1} dB is not 6 dB below middle {middle:.1} dB"
    );
    assert!(middle > -50.0, "soundtrack is silent at {middle:.1} dB");
}

#[test]
fn offline_assets_are_listed_and_relink_is_validated() {
    let f = fixtures();
    let tools = Toolchain::detect().expect("ffmpeg/ffprobe");
    let mut project = empty_project();
    let (one, _, _, _) = import_all(&mut project, &f, &tools);

    // Move the source away: the library must report exactly one offline asset.
    let moved = f.dir.path().join("moved-one.mp4");
    std::fs::rename(&f.one, &moved).expect("move source offline");
    let missing = project.missing_assets(f.dir.path());
    assert_eq!(missing.len(), 1, "one asset should be offline");
    assert_eq!(missing[0].id, one.id);

    // A relink to a path ffprobe cannot read fails and changes nothing.
    let stored_before = project.media_by_id(&one.id).unwrap().path.clone();
    let bad = relink_media(
        &mut project,
        f.dir.path(),
        &tools,
        &one.id,
        &f.dir.path().join("does-not-exist.mp4"),
    )
    .expect_err("relink to a missing file must fail");
    assert!(!format!("{bad:#}").is_empty());
    assert_eq!(project.media_by_id(&one.id).unwrap().path, stored_before);

    // A valid relink is probed with ffprobe and refreshes the stored metadata.
    let info = relink_media(&mut project, f.dir.path(), &tools, &one.id, &moved)
        .expect("relink to a valid file");
    assert_eq!(info.kind, MediaKind::Video);
    assert_eq!(info.width, Some(640));
    assert_eq!(info.height, Some(360));
    let relinked = project.media_by_id(&one.id).unwrap();
    assert_eq!(relinked.duration, one.duration);
    assert_eq!(relinked.width, Some(640));
    assert!(
        project.missing_assets(f.dir.path()).is_empty(),
        "the asset must be online after relink"
    );
}

#[test]
fn cli_export_succeeds_and_blocks_missing_media_without_partial_output() {
    let f = fixtures();
    let tools = Toolchain::detect().expect("ffmpeg/ffprobe");
    let mut project = empty_project();
    let (one, _, _, _) = import_all(&mut project, &f, &tools);
    let mut clip = default_clip_for(&one, 0.0);
    clip.out_point = 1.5;
    project.primary_video_track_mut().unwrap().clips.push(clip);
    let project_path = f.dir.path().join("cli.splice");
    save_project(&project_path, &mut project, Some(f.dir.path())).expect("save");

    let output = f.dir.path().join("cli.mp4");
    let status = Command::new(env!("CARGO_BIN_EXE_splice"))
        .args([
            "export",
            "--project",
            project_path.to_str().unwrap(),
            "--preset",
            "480p",
            "--output",
            output.to_str().unwrap(),
        ])
        .status()
        .expect("run cli export");
    assert!(status.success(), "CLI export should exit zero");
    assert!(output.is_file());
    let probe = tools.probe(&output).expect("probe cli output");
    assert_eq!(probe.width, Some(854));
    assert_eq!(probe.height, Some(480));

    // Take the source offline and retry. The CLI must exit non-zero, explain
    // the block and leave no new output or temp file behind.
    let missing = f.dir.path().join("missing.mp4");
    std::fs::rename(&f.one, &missing).unwrap();
    let blocked_output = f.dir.path().join("blocked.mp4");
    let result = Command::new(env!("CARGO_BIN_EXE_splice"))
        .args([
            "export",
            "--project",
            project_path.to_str().unwrap(),
            "--preset",
            "480p",
            "--output",
            blocked_output.to_str().unwrap(),
        ])
        .output()
        .expect("run blocked export");
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("export blocked") && stderr.contains("missing media"),
        "unexpected stderr: {stderr}"
    );
    assert!(!blocked_output.exists(), "no partial output may be created");
    let leftovers: Vec<String> = std::fs::read_dir(f.dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".splice-export-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp exports left behind: {leftovers:?}"
    );

    let error = export_project(
        &project,
        f.dir.path(),
        &tools,
        ExportPreset::P480p,
        &blocked_output,
    )
    .expect_err("library export must also refuse missing media");
    assert!(format!("{error:#}").contains("export blocked"));
    assert!(!blocked_output.exists());
}

// ---------------------------------------------------------------------------
// Small ffmpeg/image helpers.
// ---------------------------------------------------------------------------

const FRAME_WIDTH: u32 = 854;
const FRAME_HEIGHT: u32 = 480;

fn frame(tools: &Toolchain, video: &Path, at: f64) -> image::RgbImage {
    let output = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
        .args(["-ss", &format!("{at:.3}")])
        .arg("-i")
        .arg(video)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("extract frame");
    assert!(output.status.success(), "frame extraction failed");
    let expected = (FRAME_WIDTH * FRAME_HEIGHT * 3) as usize;
    assert_eq!(output.stdout.len(), expected, "unexpected raw frame size");
    image::RgbImage::from_raw(FRAME_WIDTH, FRAME_HEIGHT, output.stdout).expect("rgb image")
}

fn mean_abs_diff(a: &image::RgbImage, b: &image::RgbImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let mut total = 0.0f64;
    let mut count = 0.0f64;
    // Compare the central 50% where the title and camera move are visible.
    for y in (FRAME_HEIGHT / 4)..(FRAME_HEIGHT * 3 / 4) {
        for x in (FRAME_WIDTH / 4)..(FRAME_WIDTH * 3 / 4) {
            let pa = a.get_pixel(x, y);
            let pb = b.get_pixel(x, y);
            for channel in 0..3 {
                total += (pa[channel] as f64 - pb[channel] as f64).abs();
                count += 1.0;
            }
        }
    }
    total / count
}

fn mean_saturation(image: &image::RgbImage) -> f64 {
    let mut total = 0.0f64;
    let mut count = 0.0f64;
    for pixel in image.pixels() {
        let max = pixel[0].max(pixel[1]).max(pixel[2]) as f64;
        let min = pixel[0].min(pixel[1]).min(pixel[2]) as f64;
        total += max - min;
        count += 1.0;
    }
    total / count
}

fn mean_volume(tools: &Toolchain, video: &Path, start: f64, duration: f64) -> f64 {
    let output = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "info"])
        .args(["-ss", &format!("{start:.3}")])
        .arg("-i")
        .arg(video)
        .args(["-t", &format!("{duration:.3}"), "-map", "0:a:0"])
        .args(["-af", "volumedetect", "-f", "null", "-"])
        .output()
        .expect("measure volume");
    assert!(output.status.success(), "volumedetect failed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .find(|line| line.contains("mean_volume:"))
        .unwrap_or_else(|| panic!("no mean_volume in {stderr}"));
    line.split("mean_volume:")
        .nth(1)
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_else(|| panic!("could not parse {line}"))
}

fn ffprobe_field(video: &Path, entry: &str) -> Option<String> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", entry])
        .args(["-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(video)
        .output()
        .expect("ffprobe field");
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

#[test]
fn titles_on_the_primary_video_track_are_rejected_on_save() {
    let dir = TempDir::new().unwrap();
    let mut project = empty_project();
    let clip = splice::model::Clip {
        id: "title-on-video".into(),
        name: "bad".into(),
        media_id: None,
        start: 0.0,
        in_point: 0.0,
        out_point: 2.0,
        effects: Vec::new(),
        transition: None,
        audio: splice::model::AudioParams::default(),
        title: Some(Title::opening("Misplaced")),
        motion: splice::model::ClipMotion::default(),
    };
    project.primary_video_track_mut().unwrap().clips.push(clip);
    let path = dir.path().join("bad.splice");
    let error = save_project(&path, &mut project, Some(dir.path()))
        .expect_err("a title on the video track cannot be rendered")
        .to_string();
    assert!(error.contains("Overlay"), "{error}");
    assert!(!path.exists());
}
