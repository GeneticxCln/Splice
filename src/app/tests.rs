//! Headless interaction tests: the whole UI runs against a plain
//! `egui::Context`, with input injected as events.

use std::process::Command;
use std::time::Duration;

use super::*;
use crate::model::{Clip, MediaAsset, MediaKind, default_clip_for};
use crate::project_io::save_project;

fn new_app() -> (SpliceApp, egui::Context) {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let app = SpliceApp::from_options(AppOptions::default()).expect("app");
    (app, ctx)
}

fn raw_input(events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(1600.0, 1000.0),
        )),
        events,
        modifiers,
        ..Default::default()
    }
}

fn run(app: &mut SpliceApp, ctx: &egui::Context, input: egui::RawInput) -> egui::FullOutput {
    ctx.run(input, |ctx| app.run_frame(ctx))
}

fn idle(app: &mut SpliceApp, ctx: &egui::Context) {
    run(app, ctx, raw_input(Vec::new(), egui::Modifiers::NONE));
}

fn key(key: Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }]
}

fn ctrl() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    }
}

fn offline_asset(id: &str, kind: MediaKind) -> MediaAsset {
    MediaAsset {
        id: id.into(),
        name: id.into(),
        path: format!("/nonexistent/{id}.mp4"),
        kind,
        duration: 4.0,
        width: Some(640),
        height: Some(360),
        has_audio: false,
        sample_rate: None,
        channels: None,
    }
}

/// An app whose project holds `count` clips on the primary track.
fn app_with_clips(count: usize) -> (SpliceApp, egui::Context, Vec<String>) {
    let (mut app, ctx) = new_app();
    let asset = offline_asset("media-a", MediaKind::Video);
    app.project.media.push(asset.clone());
    let mut ids = Vec::new();
    for _ in 0..count {
        let clip: Clip = default_clip_for(&asset, 0.0);
        ids.push(clip.id.clone());
        app.project
            .primary_video_track_mut()
            .unwrap()
            .clips
            .push(clip);
    }
    edit::reflow_primary_ordered(&mut app.project);
    app.refresh_missing();
    app.dirty = false;
    (app, ctx, ids)
}

#[test]
fn clean_project_is_replaced_without_asking() {
    let (mut app, ctx) = new_app();
    app.project.name = "Old".into();
    app.request(&ctx, PendingAction::NewProject);
    assert_eq!(app.dialog, Dialog::None);
    assert_eq!(app.project.name, "Untitled");
}

#[test]
fn dirty_project_asks_before_it_is_replaced() {
    let (mut app, ctx) = new_app();
    app.project.name = "Precious".into();
    app.dirty = true;
    app.request(&ctx, PendingAction::NewProject);
    assert_eq!(
        app.dialog,
        Dialog::ConfirmDiscard(PendingAction::NewProject)
    );
    assert_eq!(app.project.name, "Precious", "nothing may change yet");
    idle(&mut app, &ctx); // the dialog renders

    app.close_dialog(); // Cancel
    assert_eq!(app.dialog, Dialog::None);
    assert_eq!(app.project.name, "Precious");
    assert!(app.dirty);
}

#[test]
fn save_then_continue_runs_the_pending_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keep.splice");
    let (mut app, ctx) = new_app();
    app.project.name = "Keep me".into();
    app.project_path = Some(path.clone());
    app.project_dir = dir.path().to_path_buf();
    app.dirty = true;

    app.request(&ctx, PendingAction::NewShort);
    app.after_save = Some(PendingAction::NewShort);
    app.save_current(&ctx);

    let saved = crate::project_io::load_project(&path).expect("saved first");
    assert_eq!(saved.project.name, "Keep me");
    assert!(app.after_save.is_none());
    assert!(app.project.settings.is_short(), "pending action ran");
    assert!(!app.dirty);
}

#[test]
fn closing_the_window_with_unsaved_changes_is_intercepted() {
    let (mut app, ctx) = new_app();
    app.dirty = true;
    let mut input = raw_input(Vec::new(), egui::Modifiers::NONE);
    input
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    let output = run(&mut app, &ctx, input);

    assert_eq!(app.dialog, Dialog::ConfirmDiscard(PendingAction::Quit));
    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose)),
        "close must be cancelled: {commands:?}"
    );
}

#[test]
fn closing_a_clean_project_is_not_blocked() {
    let (mut app, ctx) = new_app();
    let mut input = raw_input(Vec::new(), egui::Modifiers::NONE);
    input
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    let output = run(&mut app, &ctx, input);
    assert_eq!(app.dialog, Dialog::None);
    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    );
}

#[test]
fn delete_key_removes_the_selected_clip_and_undo_brings_it_back() {
    let (mut app, ctx, ids) = app_with_clips(2);
    app.selected_clip = Some(ids[0].clone());
    idle(&mut app, &ctx);

    run(
        &mut app,
        &ctx,
        raw_input(
            key(Key::Delete, egui::Modifiers::NONE),
            egui::Modifiers::NONE,
        ),
    );
    assert!(app.project.clip_by_id(&ids[0]).is_none());
    assert!(app.dirty);
    assert!(app.history.can_undo());

    run(&mut app, &ctx, raw_input(key(Key::Z, ctrl()), ctrl()));
    assert!(
        app.project.clip_by_id(&ids[0]).is_some(),
        "undo restores it"
    );
    assert_eq!(app.project.primary_video_track().unwrap().clips.len(), 2);
}

#[test]
fn a_rejected_split_leaves_no_undo_entry_and_no_dirty_flag() {
    let (mut app, ctx, ids) = app_with_clips(1);
    app.selected_clip = Some(ids[0].clone());
    idle(&mut app, &ctx);
    // The playhead is at 0, on the clip's edge.
    run(
        &mut app,
        &ctx,
        raw_input(key(Key::S, egui::Modifiers::NONE), egui::Modifiers::NONE),
    );
    assert!(app.status_error, "status: {}", app.status);
    assert!(!app.history.can_undo());
    assert!(!app.dirty);
}

#[test]
fn export_is_refused_while_media_is_offline() {
    let (mut app, _ctx, _) = app_with_clips(1);
    if app.tools.is_none() {
        return; // ffmpeg is required for the rest of this path
    }
    app.export_path = "/tmp/never-written.mp4".into();
    app.start_export();
    assert!(app.export_rx.is_none(), "no export may start");
    let error = app.dialog_error.clone().unwrap_or_default();
    assert!(error.contains("offline"), "{error}");
}

#[test]
fn autosave_runs_after_the_interval_and_only_while_dirty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("work.splice");
    let (mut app, _ctx) = new_app();
    app.project_path = Some(path.clone());
    app.project_dir = dir.path().to_path_buf();
    let autosave = crate::autosave::autosave_path(&path);

    app.dirty = true;
    app.autosave_tick();
    assert!(!autosave.exists(), "too soon");

    app.last_autosave = Instant::now() - Duration::from_secs(60);
    app.dirty = false;
    app.autosave_tick();
    assert!(!autosave.exists(), "clean projects are not autosaved");

    app.dirty = true;
    app.autosave_tick();
    assert!(autosave.exists());

    app.last_autosave = Instant::now() - Duration::from_secs(60);
    app.recovery_unresolved = true;
    std::fs::remove_file(&autosave).unwrap();
    app.autosave_tick();
    assert!(
        !autosave.exists(),
        "must not overwrite an unresolved recovery"
    );
}

#[test]
fn opening_a_project_with_a_newer_autosave_offers_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crashy.splice");
    let mut saved = Project::new("Before crash");
    save_project(&path, &mut saved, Some(dir.path())).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let mut edited = saved.clone();
    edited.name = "After crash".into();
    crate::autosave::write_autosave(&path, &edited, dir.path()).unwrap();

    let (mut app, ctx) = new_app();
    app.open_project(&path);
    let Dialog::Recover(recovery) = app.dialog.clone() else {
        panic!("expected the recovery dialog, got {:?}", app.dialog);
    };
    assert!(app.recovery_unresolved);
    idle(&mut app, &ctx);

    app.recover_from(&recovery);
    assert_eq!(app.project.name, "After crash");
    assert!(app.dirty, "recovered edits still need saving");
    assert!(!app.recovery_unresolved);
}

#[test]
fn importing_probes_on_a_worker_thread_and_adds_the_asset() {
    let (mut app, ctx) = new_app();
    if app.tools.is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("tone.wav");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
        .arg(&media)
        .status()
        .unwrap();
    assert!(status.success());
    app.project_dir = dir.path().to_path_buf();

    app.import_paths(&ctx, vec![media]);
    assert!(
        app.import_rx.is_some(),
        "import must be in flight, not done"
    );
    assert!(app.project.media.is_empty());

    let started = Instant::now();
    while app.import_rx.is_some() && started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(20));
        app.poll_import();
    }
    assert_eq!(app.project.media.len(), 1);
    assert_eq!(app.project.media[0].path, "tone.wav", "stored relative");
    assert!(app.dirty);
    assert!(app.history.can_undo());
}

#[test]
fn every_dialog_and_a_busy_project_render_without_panicking() {
    let (mut app, ctx, ids) = app_with_clips(3);
    app.selected_clip = Some(ids[1].clone());
    app.project.add_title(Title::opening("Hello"), 0.5, 2.0);
    let dialogs = [
        Dialog::About,
        Dialog::Export,
        Dialog::Import,
        Dialog::Open,
        Dialog::SaveAs,
        Dialog::AutoMovie,
        Dialog::Title,
        Dialog::Voiceover,
        Dialog::Missing,
        Dialog::Relink("media-a".into()),
        Dialog::ConfirmDiscard(PendingAction::Quit),
        Dialog::Recover(PathBuf::from("/nonexistent.splice")),
    ];
    idle(&mut app, &ctx);
    for dialog in dialogs {
        app.open_dialog(dialog.clone());
        idle(&mut app, &ctx);
        idle(&mut app, &ctx);
        app.close_dialog();
    }
    let title_id = app.project.title_clips()[0].id.clone();
    app.selected_clip = Some(title_id);
    idle(&mut app, &ctx);
}

#[test]
fn chosen_files_fill_the_matching_dialog_field() {
    let (mut app, ctx) = new_app();
    let send = |app: &mut SpliceApp, target, outcome: PickOutcome| {
        let (tx, rx) = mpsc::channel();
        tx.send(outcome).unwrap();
        app.pick_rx = Some((rx, target));
    };

    send(
        &mut app,
        PickTarget::Open,
        Ok(vec![PathBuf::from("/m/a.splice")]),
    );
    app.poll_pick(&ctx);
    assert_eq!(app.open_path, "/m/a.splice");
    assert!(app.pick_rx.is_none());

    send(
        &mut app,
        PickTarget::SaveAs,
        Ok(vec![PathBuf::from("/m/b.splice")]),
    );
    app.poll_pick(&ctx);
    assert_eq!(app.save_path, "/m/b.splice");

    send(
        &mut app,
        PickTarget::Export,
        Ok(vec![PathBuf::from("/m/c.mp4")]),
    );
    app.poll_pick(&ctx);
    assert_eq!(app.export_path, "/m/c.mp4");

    send(
        &mut app,
        PickTarget::Relink,
        Ok(vec![PathBuf::from("/m/d.mp4")]),
    );
    app.poll_pick(&ctx);
    assert_eq!(app.relink_path, "/m/d.mp4");

    // Cancelling leaves the field alone.
    send(&mut app, PickTarget::Open, Ok(Vec::new()));
    app.poll_pick(&ctx);
    assert_eq!(app.open_path, "/m/a.splice");

    send(&mut app, PickTarget::Open, Err("chooser exploded".into()));
    app.poll_pick(&ctx);
    assert_eq!(app.dialog_error.as_deref(), Some("chooser exploded"));
}

#[test]
fn browse_buttons_render_in_every_path_dialog() {
    let (mut app, ctx) = new_app();
    app.picker = Some(filedialog::Backend::Zenity);
    for dialog in [
        Dialog::Open,
        Dialog::SaveAs,
        Dialog::Import,
        Dialog::Export,
        Dialog::Relink("missing".into()),
    ] {
        app.open_dialog(dialog);
        idle(&mut app, &ctx);
        app.close_dialog();
    }
}

#[test]
fn overlay_inspector_with_placement_renders() {
    let (mut app, ctx, ids) = app_with_clips(2);
    edit::move_clip_to_track(&mut app.project, &ids[1], "track-overlay-1").unwrap();
    app.project.clip_by_id_mut(&ids[1]).unwrap().1.placement =
        Some(crate::model::OverlayPlacement {
            scale: 0.4,
            x: 0.5,
            y: -0.5,
        });
    app.selected_clip = Some(ids[1].clone());
    idle(&mut app, &ctx);
    idle(&mut app, &ctx);
    // And the primary clip's inspector offers the move.
    app.selected_clip = Some(ids[0].clone());
    idle(&mut app, &ctx);
    assert_eq!(
        edit::valid_destinations(&app.project, &ids[0]).len(),
        1,
        "primary clip can move to the overlay"
    );
}

fn run_until(
    app: &mut SpliceApp,
    ctx: &egui::Context,
    what: &str,
    mut done: impl FnMut(&SpliceApp) -> bool,
) {
    let started = Instant::now();
    while !done(app) {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "timed out waiting for {what}"
        );
        idle(app, ctx);
        std::thread::sleep(Duration::from_millis(30));
    }
}

#[test]
fn a_long_timeline_previews_one_slice_and_follows_the_playhead() {
    let (mut app, ctx, _) = app_with_clips(8); // 8 x 4 s = 32 s
    if app.tools.is_none() {
        return;
    }
    // Stretch the timeline past the full-render limit.
    for clip in &mut app.project.primary_video_track_mut().unwrap().clips {
        clip.out_point = 10.0;
    }
    edit::reflow_primary_ordered(&mut app.project); // 80 s
    app.mark_edited();
    assert!(app.duration() > crate::window::FULL_RENDER_LIMIT);

    run_until(&mut app, &ctx, "the first slice", |app| {
        app.preview.as_ref().is_some_and(|p| p.window().is_some())
    });
    assert_eq!(app.preview.as_ref().unwrap().window(), Some((0.0, 30.0)));

    app.seek_to(50.0);
    run_until(&mut app, &ctx, "the slice around 50 s", |app| {
        app.preview.as_ref().unwrap().window() == Some((45.0, 75.0))
    });
    run_until(&mut app, &ctx, "a frame from that slice", |app| {
        app.preview.as_ref().unwrap().texture().is_some()
    });
    assert!(app.preview.as_ref().unwrap().playhead > 49.9);

    // Editing the project re-renders the slice at the playhead, not at zero.
    app.project
        .primary_video_track_mut()
        .unwrap()
        .clips
        .truncate(7);
    edit::reflow_primary_ordered(&mut app.project); // 70 s
    app.mark_edited();
    run_until(&mut app, &ctx, "the re-rendered slice", |app| {
        // Near the end the slice backs up to stay 30 s long.
        app.preview.as_ref().unwrap().window() == Some((40.0, 70.0))
    });
}

#[test]
fn playback_carries_on_into_the_next_slice() {
    let (mut app, ctx, _) = app_with_clips(8);
    if app.tools.is_none() {
        return;
    }
    for clip in &mut app.project.primary_video_track_mut().unwrap().clips {
        clip.out_point = 10.0;
    }
    edit::reflow_primary_ordered(&mut app.project);
    app.mark_edited();
    run_until(&mut app, &ctx, "the first slice", |app| {
        app.preview.as_ref().is_some_and(|p| {
            p.window() == Some((0.0, 30.0)) && p.texture().is_some() && !p.is_rendering()
        })
    });

    app.seek_to(28.0);
    idle(&mut app, &ctx);
    let total = app.duration();
    app.preview.as_mut().unwrap().toggle_playback(total);
    assert!(app.preview.as_ref().unwrap().playing);

    // Two seconds later the slice ends; the next one is rendered and playback
    // resumes in it.
    run_until(&mut app, &ctx, "playback in the second slice", |app| {
        let preview = app.preview.as_ref().unwrap();
        preview.playing && preview.window().is_some_and(|(start, _)| start > 0.0)
    });
    let preview = app.preview.as_ref().unwrap();
    assert!(preview.playhead >= 29.9, "resumed at {}", preview.playhead);
    assert_eq!(preview.window(), Some((25.0, 55.0)));
}
