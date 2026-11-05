use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Key, Layout, Rect, RichText, Sense, Stroke, Ui, Vec2,
};

use crate::audio::{Recorder, RecordingStats};
use crate::automovie::{self, AutoMovieOptions};
use crate::catalog::{self, EFFECTS, FONTS, PALETTE, TITLE_STYLES, TRANSITIONS};
use crate::edit;
use crate::export::{export_project, ExportReport};
use crate::ffmpeg::Toolchain;
use crate::history::History;
use crate::library;
use crate::model::{
    new_id, resolve_media_path, AudioParams, MediaAsset, MediaKind, Project, Title, TrackKind,
    Transition,
};
use crate::presets::ExportPreset;
use crate::preview::Preview;
use crate::project_io::{load_project, save_project};
use crate::theme;
use crate::timeline_ui::{self, TimelineAction, TimelineCommand, TimelineUiState, TimelineView};

#[derive(Debug, Clone, Default)]
pub struct AppOptions {
    pub project: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub exit_after_screenshot: bool,
    pub playhead: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Dialog {
    None,
    Open,
    SaveAs,
    Import,
    Export,
    AutoMovie,
    Voiceover,
    Title,
    Relink(String),
    Missing,
    About,
}

struct VoiceSession {
    recorder: Recorder,
    start_time: f64,
}

type ExportOutcome = std::result::Result<ExportReport, String>;

pub struct SpliceApp {
    project: Project,
    project_path: Option<PathBuf>,
    project_dir: PathBuf,
    tools: Option<Toolchain>,
    tool_error: Option<String>,

    preview: Option<Preview>,
    preview_error: Option<String>,
    timeline: TimelineUiState,
    selected_clip: Option<String>,
    selected_media: Option<String>,
    missing: HashSet<String>,

    dirty: bool,
    history: History,

    status: String,
    status_error: bool,
    dialog: Dialog,
    dialog_error: Option<String>,

    open_path: String,
    save_path: String,
    import_path: String,
    relink_path: String,

    export_preset: ExportPreset,
    export_path: String,
    export_rx: Option<Receiver<ExportOutcome>>,
    export_started: Option<Instant>,

    automovie: AutoMovieOptions,
    automovie_error: Option<String>,
    title_draft: Title,
    title_duration: f64,
    title_error: Option<String>,

    voice: Option<VoiceSession>,
    voice_error: Option<String>,
    voice_stats: Option<RecordingStats>,
    voice_device: Option<String>,

    screenshot_path: Option<PathBuf>,
    screenshot_requested: bool,
    screenshot_saved: bool,
    exit_after_screenshot: bool,
    started: Instant,
    frame_count: u64,
    drag_checkpoint: Option<String>,
}

impl SpliceApp {
    pub fn new(cc: &eframe::CreationContext<'_>, options: AppOptions) -> Result<Self> {
        theme::apply(&cc.egui_ctx);
        let initial_playhead = options.playhead;

        let (tools, tool_error) = match Toolchain::detect() {
            Ok(tools) => (Some(tools), None),
            Err(error) => (None, Some(format!("{error:#}"))),
        };

        let (project, project_path) = match &options.project {
            Some(path) => {
                let loaded = load_project(path)
                    .with_context(|| format!("could not open {}", path.display()))?;
                (loaded.project, Some(loaded.path))
            }
            None => (Project::new("Untitled"), None),
        };
        let project_dir = project_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let mut app = Self {
            project,
            project_path,
            project_dir,
            tools,
            tool_error,
            preview: None,
            preview_error: None,
            timeline: TimelineUiState::new(),
            selected_clip: None,
            selected_media: None,
            missing: HashSet::new(),
            dirty: false,
            history: History::default(),
            status: String::new(),
            status_error: false,
            dialog: Dialog::None,
            dialog_error: None,
            open_path: String::new(),
            save_path: String::new(),
            import_path: String::new(),
            relink_path: String::new(),
            export_preset: ExportPreset::P1080p,
            export_path: String::new(),
            export_rx: None,
            export_started: None,
            automovie: AutoMovieOptions::default(),
            automovie_error: None,
            title_draft: Title::opening("Untitled"),
            title_duration: 4.0,
            title_error: None,
            voice: None,
            voice_error: None,
            voice_stats: None,
            voice_device: None,
            screenshot_path: options.screenshot.map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("."))
                        .join(path)
                }
            }),
            screenshot_requested: false,
            screenshot_saved: false,
            exit_after_screenshot: options.exit_after_screenshot,
            started: Instant::now(),
            frame_count: 0,
            drag_checkpoint: None,
        };

        app.open_path = app
            .project_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        app.save_path = app.suggested_save_path();
        app.import_path = app.project_dir.display().to_string();
        app.refresh_missing();
        app.selected_clip = app
            .project
            .primary_video_track()
            .and_then(|track| track.clips.first())
            .map(|clip| clip.id.clone())
            .or_else(|| {
                app.project
                    .tracks
                    .iter()
                    .flat_map(|track| track.clips.iter())
                    .next()
                    .map(|clip| clip.id.clone())
            });
        app.title_draft = Title::opening(&app.project.name);

        match Preview::new() {
            Ok(mut preview) => {
                preview.invalidate();
                app.preview = Some(preview);
            }
            Err(error) => app.preview_error = Some(format!("{error:#}")),
        }
        if initial_playhead > 0.0 {
            app.seek_to(initial_playhead);
        }

        app.status = match &app.tools {
            Some(_) => format!(
                "Ready — {} media · {} clips · ffmpeg detected",
                app.project.media.len(),
                app.clip_count()
            ),
            None => "ffmpeg/ffprobe were not found on PATH; preview and export are unavailable"
                .to_string(),
        };
        app.status_error = app.tools.is_none();
        Ok(app)
    }

    fn clip_count(&self) -> usize {
        self.project
            .tracks
            .iter()
            .map(|track| track.clips.len())
            .sum()
    }

    fn playhead(&self) -> f64 {
        self.preview
            .as_ref()
            .map(|preview| preview.playhead)
            .unwrap_or(0.0)
    }

    fn duration(&self) -> f64 {
        self.project.timeline_duration()
    }

    fn suggested_save_path(&self) -> String {
        if let Some(path) = &self.project_path {
            return path.display().to_string();
        }
        let stem = sanitize_file_stem(&self.project.name);
        self.project_dir
            .join(format!("{stem}.splice"))
            .display()
            .to_string()
    }

    fn suggested_export_path(&self) -> String {
        let stem = self
            .project_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| sanitize_file_stem(&self.project.name));
        let directory = self
            .project_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.project_dir.clone());
        directory
            .join(format!("{stem}-{}.mp4", self.export_preset.label()))
            .display()
            .to_string()
    }

    fn refresh_missing(&mut self) {
        self.missing.clear();
        for asset in &self.project.media {
            if !resolve_media_path(&self.project_dir, asset).is_file() {
                self.missing.insert(asset.id.clone());
            }
        }
    }

    fn missing_assets(&self) -> Vec<&MediaAsset> {
        self.project
            .media
            .iter()
            .filter(|asset| self.missing.contains(&asset.id))
            .collect()
    }

    fn invalidate_preview(&mut self) {
        if let Some(preview) = &mut self.preview {
            preview.invalidate();
        }
    }

    fn mark_edited(&mut self) {
        self.dirty = true;
        self.refresh_missing();
        self.invalidate_preview();
        if let Some(id) = &self.selected_clip {
            if self.project.clip_by_id(id).is_none() {
                self.selected_clip = None;
            }
        }
    }

    fn push_undo(&mut self) {
        self.history.checkpoint(self.project.clone());
        self.dirty = true;
    }

    fn push_undo_snapshot(&mut self, snapshot: Project) {
        self.history.checkpoint(snapshot);
        self.dirty = true;
    }

    fn undo(&mut self) {
        if self.history.undo(&mut self.project) {
            self.dirty = true;
            self.mark_edited();
            self.status = "Undo".to_string();
            self.status_error = false;
        }
    }

    fn redo(&mut self) {
        if self.history.redo(&mut self.project) {
            self.dirty = true;
            self.mark_edited();
            self.status = "Redo".to_string();
            self.status_error = false;
        }
    }

    fn set_status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.status_error = error;
    }

    fn seek_to(&mut self, time: f64) {
        let duration = self.duration();
        if let Some(preview) = &mut self.preview {
            preview.set_playhead(time.clamp(0.0, duration.max(0.0)), duration);
        }
    }

    fn open_dialog(&mut self, dialog: Dialog) {
        self.dialog_error = None;
        self.automovie_error = None;
        self.title_error = None;
        match &dialog {
            Dialog::Open => {
                if self.open_path.trim().is_empty() {
                    self.open_path = self
                        .project_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| self.project_dir.display().to_string());
                }
            }
            Dialog::SaveAs => {
                self.save_path = self.suggested_save_path();
            }
            Dialog::Import => {
                if self.import_path.trim().is_empty() {
                    self.import_path = self.project_dir.display().to_string();
                }
            }
            Dialog::Export => {
                self.export_path = self.suggested_export_path();
            }
            Dialog::AutoMovie => {
                if self.automovie.opening_text.trim().is_empty() {
                    self.automovie.opening_text = self.project.name.clone();
                }
            }
            Dialog::Title => {
                self.title_draft = Title::opening(&self.project.name);
                self.title_draft.text = "A New Story".to_string();
                self.title_duration = 4.0;
            }
            Dialog::Relink(id) => {
                if let Some(asset) = self.project.media_by_id(id) {
                    self.relink_path = resolve_media_path(&self.project_dir, asset)
                        .display()
                        .to_string();
                }
            }
            Dialog::Voiceover => {
                self.voice_stats = None;
                self.voice_error = None;
                if self.voice_device.is_none() {
                    self.voice_device = crate::audio::describe_default_input()
                        .map_err(|error| format!("{error:#}"))
                        .ok();
                }
            }
            _ => {}
        }
        self.dialog = dialog;
    }

    fn close_dialog(&mut self) {
        if let Some(session) = self.voice.take() {
            let _ = session.recorder.stop();
        }
        self.dialog = Dialog::None;
        self.dialog_error = None;
        self.automovie_error = None;
        self.title_error = None;
        self.voice_error = None;
    }

    fn save_current(&mut self, ctx: &egui::Context) {
        let Some(path) = self.project_path.clone() else {
            self.open_dialog(Dialog::SaveAs);
            return;
        };
        self.save_to(ctx, &path);
    }

    fn save_to(&mut self, ctx: &egui::Context, path: &Path) {
        let mut destination = path.to_path_buf();
        if destination
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("splice"))
            != Some(true)
        {
            destination.set_extension("splice");
        }
        match save_project(
            &destination,
            &mut self.project,
            Some(self.project_dir.as_path()),
        ) {
            Ok(()) => {
                let absolute = if destination.is_absolute() {
                    destination.clone()
                } else {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::from("."))
                        .join(&destination)
                };
                self.project_path = Some(absolute.clone());
                self.project_dir = absolute
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| self.project_dir.clone());
                self.save_path = absolute.display().to_string();
                self.open_path = absolute.display().to_string();
                self.dirty = false;
                self.history.break_coalescing();
                self.refresh_missing();
                self.set_status(format!("Saved project · {}", absolute.display()), false);
                self.dialog = Dialog::None;
                self.dialog_error = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title("Splice".to_string()));
            }
            Err(error) => {
                self.dialog_error = Some(format!("{error:#}"));
            }
        }
    }

    fn open_project(&mut self, path: &Path) {
        match load_project(path) {
            Ok(loaded) => {
                self.project = loaded.project;
                self.project_path = Some(loaded.path.clone());
                self.project_dir = loaded
                    .path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| self.project_dir.clone());
                self.selected_clip = self
                    .project
                    .primary_video_track()
                    .and_then(|track| track.clips.first())
                    .map(|clip| clip.id.clone());
                self.selected_media = None;
                self.history.clear();
                self.dirty = false;
                self.drag_checkpoint = None;
                self.save_path = self.suggested_save_path();
                self.open_path = loaded.path.display().to_string();
                self.import_path = self.project_dir.display().to_string();
                self.refresh_missing();
                self.invalidate_preview();
                self.set_status(
                    format!(
                        "Opened {} · {} media · {} clips",
                        loaded.path.display(),
                        self.project.media.len(),
                        self.clip_count()
                    ),
                    false,
                );
                self.dialog = Dialog::None;
                self.dialog_error = None;
            }
            Err(error) => {
                self.dialog_error = Some(format!("{error:#}"));
                self.status_error = true;
            }
        }
    }

    fn new_project(&mut self) {
        self.project = Project::new("Untitled");
        self.project_path = None;
        self.project_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        self.selected_clip = None;
        self.selected_media = None;
        self.history.clear();
        self.dirty = false;
        self.drag_checkpoint = None;
        self.save_path = self.suggested_save_path();
        self.import_path = self.project_dir.display().to_string();
        self.refresh_missing();
        self.invalidate_preview();
        self.title_draft = Title::opening("Untitled");
        self.set_status("New project", false);
        self.dialog = Dialog::None;
    }

    fn import_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if let Some(splice) = paths.iter().find(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("splice"))
                .unwrap_or(false)
        }) {
            self.open_project(splice);
            return;
        }
        let Some(tools) = self.tools.clone() else {
            self.set_status("ffprobe is required to import media", true);
            return;
        };
        self.push_undo();
        let mut imported = 0usize;
        let mut errors = Vec::new();
        for path in paths {
            match library::import_media(
                &mut self.project,
                Some(self.project_dir.as_path()),
                &tools,
                &path,
            ) {
                Ok(asset) => {
                    imported += 1;
                    self.selected_media = Some(asset.id);
                }
                Err(error) => errors.push(format!("{}: {error:#}", path.display())),
            }
        }
        if imported > 0 {
            self.mark_edited();
            self.set_status(
                format!(
                    "Imported {imported} file(s) · {} media in library",
                    self.project.media.len()
                ),
                false,
            );
        }
        if !errors.is_empty() {
            self.set_status(format!("Import failed — {}", errors.join(" · ")), true);
            if self.dialog == Dialog::Import {
                self.dialog_error = Some(errors.join("\n"));
            }
        } else if self.dialog == Dialog::Import {
            self.dialog = Dialog::None;
        }
    }

    fn add_media_to(&mut self, asset_id: &str, placement: Placement) {
        let Some(asset) = self.project.media_by_id(asset_id).cloned() else {
            return;
        };
        if self.missing.contains(asset_id) {
            self.set_status(
                format!(
                    "{} is offline — relink it before adding to the timeline",
                    asset.name
                ),
                true,
            );
            return;
        }
        self.push_undo();
        let playhead = self.playhead();
        let result = match placement {
            Placement::Timeline => edit::add_asset_to_timeline(&mut self.project, &asset),
            Placement::Overlay => {
                edit::add_asset_to_overlay(&mut self.project, &asset, playhead).map(Some)
            }
            Placement::Music => {
                let result = edit::add_asset_to_timeline(&mut self.project, &asset);
                if let Ok(Some(id)) = &result {
                    if let Some((_, clip)) = self.project.clip_by_id_mut(id) {
                        clip.start = playhead.max(0.0);
                    }
                }
                result
            }
            Placement::Voice => {
                edit::add_asset_to_voiceover(&mut self.project, &asset, playhead).map(Some)
            }
        };
        match result {
            Ok(id) => {
                if let Some(id) = id {
                    if let Some((_, clip)) = self.project.clip_by_id(&id) {
                        let start = clip.start;
                        self.selected_clip = Some(id.clone());
                        self.seek_to(start);
                    }
                }
                self.selected_media = Some(asset_id.to_string());
                self.mark_edited();
                self.set_status(
                    format!(
                        "Added {} to {}",
                        asset.name,
                        match placement {
                            Placement::Timeline => "Video 1",
                            Placement::Overlay => "Overlay",
                            Placement::Music => "Music",
                            Placement::Voice => "Voiceover",
                        }
                    ),
                    false,
                );
            }
            Err(error) => self.set_status(format!("Could not add clip: {error:#}"), true),
        }
    }

    fn add_title(&mut self) {
        let title = self.title_draft.clone();
        let duration = self.title_duration.clamp(0.5, 60.0);
        let start = self.playhead();
        self.push_undo();
        let id = edit::add_title_clip(&mut self.project, title, start, duration);
        self.selected_clip = Some(id);
        self.mark_edited();
        self.set_status("Title added to the Overlay track", false);
        self.dialog = Dialog::None;
        self.title_error = None;
    }

    fn delete_selected(&mut self) {
        let Some(id) = self.selected_clip.clone() else {
            return;
        };
        self.push_undo();
        if edit::delete_clip(&mut self.project, &id) {
            self.selected_clip = None;
            self.mark_edited();
            self.set_status("Clip deleted", false);
        }
    }

    fn duplicate_selected(&mut self) {
        let Some(id) = self.selected_clip.clone() else {
            return;
        };
        self.push_undo();
        match edit::duplicate_clip(&mut self.project, &id) {
            Ok(new_id) => {
                self.selected_clip = new_id;
                self.mark_edited();
                self.set_status("Clip duplicated", false);
            }
            Err(error) => self.set_status(format!("{error:#}"), true),
        }
    }

    fn split_selected(&mut self) {
        let Some(id) = self.selected_clip.clone() else {
            return;
        };
        let at = self.playhead();
        self.push_undo();
        match edit::split_clip(&mut self.project, &id, at) {
            Ok(new_id) => {
                self.selected_clip = new_id;
                self.mark_edited();
                self.set_status(format!("Split at {}", theme::format_time(at)), false);
            }
            Err(error) => {
                self.set_status(format!("{error:#}"), true);
            }
        }
    }

    fn handle_timeline_action(&mut self, action: TimelineAction) {
        match action {
            TimelineAction::SelectClip(id) => {
                self.selected_media = self
                    .project
                    .clip_by_id(&id)
                    .and_then(|(_, clip)| clip.media_id.clone());
                self.selected_clip = Some(id);
            }
            TimelineAction::ClearSelection => self.selected_clip = None,
            TimelineAction::Seek(time) => self.seek_to(time),
            TimelineAction::MoveClip { clip_id, start } => {
                if self.drag_checkpoint.as_deref() != Some(clip_id.as_str()) {
                    self.push_undo();
                    self.drag_checkpoint = Some(clip_id.clone());
                }
                if let Some((_, clip)) = self.project.clip_by_id_mut(&clip_id) {
                    clip.start = start.max(0.0);
                }
                self.dirty = true;
                self.invalidate_preview();
            }
            TimelineAction::FinishClipMove(clip_id) => {
                self.drag_checkpoint = None;
                self.history.break_coalescing();
                if edit::clip_track_kind(&self.project, &clip_id) == Some(TrackKind::Video) {
                    edit::sort_and_reflow_primary(&mut self.project);
                }
                self.mark_edited();
            }
            TimelineAction::TrimClip {
                clip_id,
                start,
                in_point,
                out_point,
            } => {
                if self.drag_checkpoint.as_deref() != Some(clip_id.as_str()) {
                    self.push_undo();
                    self.drag_checkpoint = Some(clip_id.clone());
                }
                if let Some((_, clip)) = self.project.clip_by_id_mut(&clip_id) {
                    clip.start = start.max(0.0);
                    clip.in_point = in_point.max(0.0);
                    clip.out_point = out_point.max(clip.in_point + 0.05);
                }
                self.dirty = true;
                self.invalidate_preview();
            }
            TimelineAction::FinishTrim(clip_id) => {
                self.drag_checkpoint = None;
                self.history.break_coalescing();
                if edit::clip_track_kind(&self.project, &clip_id) == Some(TrackKind::Video) {
                    edit::sort_and_reflow_primary(&mut self.project);
                }
                self.mark_edited();
            }
            TimelineAction::ReorderPrimary {
                clip_id,
                target_index,
            } => {
                self.push_undo();
                if edit::move_primary_clip_to_index(&mut self.project, &clip_id, target_index) {
                    self.selected_clip = Some(clip_id);
                    self.mark_edited();
                }
            }
            TimelineAction::Command(command) => self.handle_timeline_command(command),
        }
    }

    fn handle_timeline_command(&mut self, command: TimelineCommand) {
        match command {
            TimelineCommand::SplitAtPlayhead(id) => {
                self.selected_clip = Some(id.clone());
                let at = self.playhead();
                self.push_undo();
                match edit::split_clip(&mut self.project, &id, at) {
                    Ok(new_id) => {
                        self.selected_clip = new_id;
                        self.mark_edited();
                    }
                    Err(error) => self.set_status(format!("{error:#}"), true),
                }
            }
            TimelineCommand::Duplicate(id) => {
                self.push_undo();
                match edit::duplicate_clip(&mut self.project, &id) {
                    Ok(new_id) => {
                        self.selected_clip = new_id;
                        self.mark_edited();
                    }
                    Err(error) => self.set_status(format!("{error:#}"), true),
                }
            }
            TimelineCommand::Delete(id) => {
                self.push_undo();
                if edit::delete_clip(&mut self.project, &id) {
                    if self.selected_clip.as_deref() == Some(id.as_str()) {
                        self.selected_clip = None;
                    }
                    self.mark_edited();
                    self.set_status("Clip deleted", false);
                }
            }
            TimelineCommand::MoveLeft(id) => {
                self.push_undo();
                if edit::move_primary_clip_by(&mut self.project, &id, -1) {
                    self.mark_edited();
                }
            }
            TimelineCommand::MoveRight(id) => {
                self.push_undo();
                if edit::move_primary_clip_by(&mut self.project, &id, 1) {
                    self.mark_edited();
                }
            }
            TimelineCommand::RemoveTransition(id) => {
                self.push_undo();
                if let Some((_, clip)) = self.project.clip_by_id_mut(&id) {
                    clip.transition = None;
                }
                edit::reflow_primary_ordered(&mut self.project);
                self.mark_edited();
            }
            TimelineCommand::SetTransition(id, kind) => {
                self.push_undo();
                if let Some((_, clip)) = self.project.clip_by_id_mut(&id) {
                    clip.transition = Some(Transition {
                        kind,
                        duration: catalog::default_transition_duration(),
                    });
                }
                edit::reflow_primary_ordered(&mut self.project);
                self.mark_edited();
                self.set_status(
                    format!("Transition set to {}", catalog::transition_name(kind)),
                    false,
                );
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    Timeline,
    Overlay,
    Music,
    Voice,
}

fn sanitize_file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches('-').to_string();
    if stem.is_empty() {
        "Untitled".to_string()
    } else {
        stem
    }
}

pub fn parse_hex_color(value: &str) -> Color32 {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() >= 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color32::from_rgb(r, g, b);
        }
    }
    Color32::from_rgb(245, 242, 234)
}

pub fn hex_color(color: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

impl eframe::App for SpliceApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;
        self.handle_events(ctx);
        self.poll_export();
        self.handle_shortcuts(ctx);

        let duration = self.duration();
        if let Some(preview) = &mut self.preview {
            preview.tick(
                ctx,
                &self.project,
                &self.project_dir,
                self.tools.as_ref(),
                duration,
            );
        }
        if self.preview_error.is_some() {
            if let Some(error) = self.preview.as_ref().and_then(Preview::diagnostic) {
                self.preview_error = Some(error);
            }
        }

        self.top_menu(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.timeline_panel(ctx);
        self.media_panel(ctx);
        self.inspector_panel(ctx);
        self.preview_panel(ctx);
        self.dialogs(ctx);
        self.request_repaints(ctx);
        self.screenshot_tick(ctx);
    }
}

impl SpliceApp {
    fn handle_events(&mut self, ctx: &egui::Context) {
        let (dropped, shots) = ctx.input(|input| {
            let dropped: Vec<PathBuf> = input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect();
            let shots: Vec<std::sync::Arc<egui::ColorImage>> = input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect();
            (dropped, shots)
        });
        if !dropped.is_empty() {
            self.import_paths(dropped);
        }
        for image in shots {
            self.save_screenshot(ctx, image.as_ref());
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.key_pressed(Key::Escape)) && self.dialog != Dialog::None {
            self.close_dialog();
            return;
        }
        if self.dialog != Dialog::None || ctx.wants_keyboard_input() {
            return;
        }
        let command = ctx.input(|input| input.modifiers.command);
        let shift = ctx.input(|input| input.modifiers.shift);
        if command && ctx.input(|input| input.key_pressed(Key::S)) {
            self.save_current(ctx);
            return;
        }
        if command && ctx.input(|input| input.key_pressed(Key::O)) {
            self.open_dialog(Dialog::Open);
            return;
        }
        if command && ctx.input(|input| input.key_pressed(Key::N)) {
            self.new_project();
            return;
        }
        if command && ctx.input(|input| input.key_pressed(Key::E)) {
            self.open_dialog(Dialog::Export);
            return;
        }
        if command && ctx.input(|input| input.key_pressed(Key::Z)) {
            if shift {
                self.redo();
            } else {
                self.undo();
            }
            return;
        }
        if command && ctx.input(|input| input.key_pressed(Key::Y)) {
            self.redo();
            return;
        }
        if ctx.input(|input| input.key_pressed(Key::Space)) {
            let duration = self.duration();
            if let Some(preview) = &mut self.preview {
                preview.toggle_playback(duration);
            }
            return;
        }
        if ctx.input(|input| input.key_pressed(Key::Delete)) {
            self.delete_selected();
            return;
        }
        if ctx.input(|input| input.key_pressed(Key::S)) {
            self.split_selected();
            return;
        }
        if ctx.input(|input| input.key_pressed(Key::D)) {
            self.duplicate_selected();
        }
    }

    fn poll_export(&mut self) {
        let mut outcome = None;
        let mut clear = false;
        if let Some(rx) = &self.export_rx {
            match rx.try_recv() {
                Ok(result) => {
                    outcome = Some(result);
                    clear = true;
                }
                Err(TryRecvError::Disconnected) => {
                    outcome = Some(Err("export worker stopped unexpectedly".to_string()));
                    clear = true;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if clear {
            self.export_rx = None;
            self.export_started = None;
        }
        if let Some(result) = outcome {
            match result {
                Ok(report) => {
                    self.set_status(
                        format!(
                            "Export complete · {} · {} · {:.1}s",
                            report.output.display(),
                            report.preset.label(),
                            report.duration
                        ),
                        false,
                    );
                    self.dialog = Dialog::None;
                    self.dialog_error = None;
                }
                Err(error) => {
                    self.dialog_error = Some(error.clone());
                    self.set_status(format!("Export failed — {error}"), true);
                }
            }
        }
    }

    fn request_repaints(&mut self, ctx: &egui::Context) {
        let mut needs_repaint = false;
        let duration = self.duration();
        if duration > 0.05 {
            if let Some(preview) = &self.preview {
                needs_repaint |=
                    preview.is_rendering() || preview.playing || preview.finish_render_hint();
            }
        }
        needs_repaint |= self.export_rx.is_some();
        needs_repaint |= self.voice.is_some();
        needs_repaint |= self.screenshot_path.is_some() && !self.screenshot_saved;
        if needs_repaint {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    fn screenshot_tick(&mut self, ctx: &egui::Context) {
        let Some(_path) = &self.screenshot_path else {
            return;
        };
        if self.screenshot_saved || self.screenshot_requested {
            if self.screenshot_requested
                && !self.screenshot_saved
                && self.started.elapsed() > Duration::from_secs(45)
            {
                eprintln!("SCREENSHOT_FAILED timeout waiting for the framebuffer capture");
                if self.exit_after_screenshot {
                    std::process::exit(1);
                }
            }
            return;
        }
        let elapsed = self.started.elapsed();
        let has_content = self.clip_count() > 0;
        let preview_ready = self
            .preview
            .as_ref()
            .map(|preview| preview.texture().is_some())
            .unwrap_or(false);
        let ready = if has_content {
            preview_ready || elapsed > Duration::from_secs(28)
        } else {
            elapsed > Duration::from_secs(2)
        };
        if ready && self.frame_count >= 12 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                "splice-gate",
            )));
            self.screenshot_requested = true;
        }
    }

    fn save_screenshot(&mut self, ctx: &egui::Context, image: &egui::ColorImage) {
        if self.screenshot_saved {
            return;
        }
        let Some(path) = self.screenshot_path.clone() else {
            return;
        };
        let width = image.width() as u32;
        let height = image.height() as u32;
        let mut rgba = Vec::with_capacity(image.pixels.len() * 4);
        for pixel in &image.pixels {
            rgba.extend_from_slice(&pixel.to_array());
        }
        let result = (|| -> Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            image::save_buffer(&path, &rgba, width, height, image::ColorType::Rgba8)
                .with_context(|| format!("write PNG {}", path.display()))?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.screenshot_saved = true;
                println!("SCREENSHOT_OK {} {}x{}", path.display(), width, height);
                if self.exit_after_screenshot {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Err(error) => {
                eprintln!("SCREENSHOT_FAILED {error:#}");
                if self.exit_after_screenshot {
                    std::process::exit(1);
                }
            }
        }
    }

    fn top_menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu")
            .exact_height(27.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(10, 2)),
            )
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New project").clicked() {
                            self.new_project();
                            ui.close_menu();
                        }
                        if ui.button("Open project…").clicked() {
                            self.open_dialog(Dialog::Open);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Save").clicked() {
                            self.save_current(ctx);
                            ui.close_menu();
                        }
                        if ui.button("Save as…").clicked() {
                            self.open_dialog(Dialog::SaveAs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Import media…").clicked() {
                            self.open_dialog(Dialog::Import);
                            ui.close_menu();
                        }
                        if ui.button("Export movie…").clicked() {
                            self.open_dialog(Dialog::Export);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        if ui
                            .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                            .clicked()
                        {
                            self.undo();
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                            .clicked()
                        {
                            self.redo();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Split at playhead").clicked() {
                            self.split_selected();
                            ui.close_menu();
                        }
                        if ui.button("Duplicate clip").clicked() {
                            self.duplicate_selected();
                            ui.close_menu();
                        }
                        if ui.button("Delete clip").clicked() {
                            self.delete_selected();
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Add", |ui| {
                        if ui.button("Title…").clicked() {
                            self.open_dialog(Dialog::Title);
                            ui.close_menu();
                        }
                        if ui.button("AutoMovie…").clicked() {
                            self.open_dialog(Dialog::AutoMovie);
                            ui.close_menu();
                        }
                        if ui.button("Voiceover…").clicked() {
                            self.open_dialog(Dialog::Voiceover);
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        if ui.button("Storyboard").clicked() {
                            self.timeline.view = TimelineView::Storyboard;
                            ui.close_menu();
                        }
                        if ui.button("Timeline").clicked() {
                            self.timeline.view = TimelineView::Timeline;
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("About Splice").clicked() {
                            self.open_dialog(Dialog::About);
                            ui.close_menu();
                        }
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (label, color) = if self.tools.is_some() {
                            ("FFMPEG READY", theme::GREEN)
                        } else {
                            ("FFMPEG MISSING", theme::DANGER)
                        };
                        let response = ui.label(
                            RichText::new(format!("● {label}"))
                                .size(9.5)
                                .strong()
                                .color(color),
                        );
                        if let Some(tools) = &self.tools {
                            response.on_hover_text(tools.version_summary());
                        } else if let Some(error) = &self.tool_error {
                            response.on_hover_text(error);
                        }
                    });
                });
            });
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar")
            .exact_height(46.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::symmetric(12, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("SPLICE")
                            .size(20.0)
                            .strong()
                            .color(theme::ACCENT),
                    );
                    ui.label(
                        RichText::new("STUDIO")
                            .size(8.5)
                            .strong()
                            .color(theme::MUTED),
                    );
                    ui.add_space(8.0);
                    ui.separator();
                    if ui.button("New").clicked() {
                        self.new_project();
                    }
                    if ui.button("Open").clicked() {
                        self.open_dialog(Dialog::Open);
                    }
                    if ui.button("Save").clicked() {
                        self.save_current(ctx);
                    }
                    if ui.button("Import").clicked() {
                        self.open_dialog(Dialog::Import);
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.redo();
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let export = egui::Button::new(
                            RichText::new("Export Movie").strong().color(theme::BG),
                        )
                        .fill(theme::ACCENT)
                        .stroke(Stroke::NONE);
                        if ui.add(export).clicked() {
                            self.open_dialog(Dialog::Export);
                        }
                        if ui.button("AutoMovie").clicked() {
                            self.open_dialog(Dialog::AutoMovie);
                        }
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(&self.project.name)
                                .size(13.0)
                                .strong()
                                .color(theme::TEXT),
                        );
                        if self.dirty {
                            ui.label(RichText::new("●").size(9.0).color(theme::ACCENT))
                                .on_hover_text("Unsaved changes");
                        }
                        ui.label(
                            RichText::new(format!("{} clips", self.clip_count()))
                                .size(11.0)
                                .color(theme::MUTED),
                        );
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(25.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::symmetric(10, 3)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let color = if self.status_error {
                        theme::DANGER
                    } else {
                        theme::GREEN
                    };
                    ui.label(RichText::new("●").size(8.0).color(color));
                    ui.label(
                        RichText::new(&self.status)
                            .size(11.0)
                            .color(if self.status_error {
                                theme::DANGER
                            } else {
                                theme::MUTED
                            }),
                    );
                    if !self.missing.is_empty() {
                        let response = ui.add(
                            egui::Button::new(
                                RichText::new(format!("{} missing media", self.missing.len()))
                                    .size(10.0)
                                    .color(theme::DANGER),
                            )
                            .small()
                            .fill(theme::DANGER.gamma_multiply(0.12)),
                        );
                        if response.clicked() {
                            self.open_dialog(Dialog::Missing);
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("OFFLINE · NO NETWORK")
                                .size(9.0)
                                .strong()
                                .color(theme::GREEN),
                        );
                        ui.separator();
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                theme::format_time(self.playhead()),
                                theme::format_time(self.duration())
                            ))
                            .size(10.5)
                            .monospace()
                            .color(theme::MUTED),
                        );
                        ui.separator();
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(10.0)
                                .color(theme::MUTED),
                        );
                    });
                });
            });
    }

    fn timeline_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("timeline-panel")
            .resizable(true)
            .default_height(306.0)
            .min_height(234.0)
            .max_height(460.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::section_label(ui, "Storyboard");
                    ui.selectable_value(&mut self.timeline.view, TimelineView::Storyboard, "Cards");
                    ui.selectable_value(
                        &mut self.timeline.view,
                        TimelineView::Timeline,
                        "Timeline",
                    );
                    ui.separator();
                    if ui.button("Add Title").clicked() {
                        self.open_dialog(Dialog::Title);
                    }
                    if ui.button("AutoMovie").clicked() {
                        self.open_dialog(Dialog::AutoMovie);
                    }
                    ui.separator();
                    let has_selection = self.selected_clip.is_some();
                    if ui
                        .add_enabled(has_selection, egui::Button::new("Split"))
                        .clicked()
                    {
                        self.split_selected();
                    }
                    if ui
                        .add_enabled(has_selection, egui::Button::new("Duplicate"))
                        .clicked()
                    {
                        self.duplicate_selected();
                    }
                    if ui
                        .add_enabled(has_selection, egui::Button::new("Delete"))
                        .clicked()
                    {
                        self.delete_selected();
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(theme::format_time(self.duration()))
                                .size(10.5)
                                .monospace()
                                .color(theme::MUTED),
                        );
                        ui.checkbox(&mut self.timeline.snap, "Snap");
                        ui.add(
                            egui::Slider::new(&mut self.timeline.zoom, 14.0..=160.0)
                                .show_value(false),
                        )
                        .on_hover_text("Timeline zoom");
                    });
                });
                ui.add_space(4.0);
                let playhead = self.playhead();
                let actions = timeline_ui::show(
                    ui,
                    &mut self.timeline,
                    &self.project,
                    &self.project_dir,
                    playhead,
                    self.selected_clip.as_deref(),
                    &self.missing,
                );
                for action in actions {
                    self.handle_timeline_action(action);
                }
            });
    }

    fn media_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("media-library")
            .resizable(true)
            .default_width(268.0)
            .width_range(236.0..=360.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::section_label(ui, "Media library");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("+ Import").clicked() {
                            self.open_dialog(Dialog::Import);
                        }
                    });
                });
                let assets = self.project.media.clone();
                if assets.is_empty() {
                    theme::muted(
                        ui,
                        "Drop video, photos or audio here, or use Import. Sources are probed with ffprobe and never modified.",
                    );
                    ui.add_space(6.0);
                    if ui.button("Import media…").clicked() {
                        self.open_dialog(Dialog::Import);
                    }
                    return;
                }

                let mut selected = self.selected_media.clone();
                let mut placement: Option<(Placement, String)> = None;
                let mut relink: Option<String> = None;
                let mut add_all = false;
                egui::ScrollArea::vertical()
                    .id_salt("media-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for asset in &assets {
                            let is_selected = selected.as_deref() == Some(asset.id.as_str());
                            let offline = self.missing.contains(&asset.id);
                            let frame = egui::Frame::new()
                                .fill(if is_selected {
                                    theme::CARD_HOVER
                                } else {
                                    theme::PANEL_RAISED
                                })
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if is_selected {
                                        theme::ACCENT.gamma_multiply(0.7)
                                    } else {
                                        theme::BORDER
                                    },
                                ))
                                .corner_radius(egui::CornerRadius::same(7))
                                .inner_margin(egui::Margin::same(8));
                            frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let color = theme::kind_color(asset.kind);
                                    let (stripe, _) = ui.allocate_exact_size(
                                        Vec2::new(3.0, 44.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        stripe,
                                        egui::CornerRadius::same(2),
                                        color,
                                    );
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            let name = ui.add(
                                                egui::Label::new(
                                                    RichText::new(&asset.name)
                                                        .size(12.5)
                                                        .strong()
                                                        .color(theme::TEXT),
                                                )
                                                .sense(Sense::click())
                                                .truncate(),
                                            );
                                            if name.clicked() {
                                                selected = Some(asset.id.clone());
                                            }
                                            theme::badge(ui, asset.kind.label(), color);
                                            if offline {
                                                theme::badge(ui, "Offline", theme::DANGER);
                                            }
                                        });
                                        let detail = match asset.kind {
                                            MediaKind::Image => format!(
                                                "{} × {} · still",
                                                asset.width.unwrap_or(0),
                                                asset.height.unwrap_or(0)
                                            ),
                                            MediaKind::Video => format!(
                                                "{} · {} × {}{}",
                                                theme::short_time(asset.duration),
                                                asset.width.unwrap_or(0),
                                                asset.height.unwrap_or(0),
                                                if asset.has_audio { " · audio" } else { "" }
                                            ),
                                            MediaKind::Audio => format!(
                                                "{} · {} Hz · {} ch",
                                                theme::short_time(asset.duration),
                                                asset.sample_rate.unwrap_or(48_000),
                                                asset.channels.unwrap_or(2)
                                            ),
                                        };
                                        ui.label(
                                            RichText::new(detail).size(10.0).color(theme::MUTED),
                                        );
                                        if offline {
                                            ui.horizontal(|ui| {
                                                if ui.small_button("Relink…").clicked() {
                                                    relink = Some(asset.id.clone());
                                                }
                                                ui.label(
                                                    RichText::new(
                                                        resolve_media_path(
                                                            &self.project_dir,
                                                            asset,
                                                        )
                                                        .display()
                                                        .to_string(),
                                                    )
                                                    .size(9.0)
                                                    .color(theme::DANGER),
                                                );
                                            });
                                        } else {
                                            ui.horizontal(|ui| {
                                                match asset.kind {
                                                    MediaKind::Video | MediaKind::Image => {
                                                        if ui.small_button("Timeline").clicked() {
                                                            placement = Some((
                                                                Placement::Timeline,
                                                                asset.id.clone(),
                                                            ));
                                                        }
                                                        if ui.small_button("Overlay").clicked() {
                                                            placement = Some((
                                                                Placement::Overlay,
                                                                asset.id.clone(),
                                                            ));
                                                        }
                                                    }
                                                    MediaKind::Audio => {
                                                        if ui.small_button("Music").clicked() {
                                                            placement = Some((
                                                                Placement::Music,
                                                                asset.id.clone(),
                                                            ));
                                                        }
                                                        if ui.small_button("Voiceover").clicked() {
                                                            placement = Some((
                                                                Placement::Voice,
                                                                asset.id.clone(),
                                                            ));
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    });
                                });
                            });
                            ui.add_space(5.0);
                        }
                    });
                self.selected_media = selected;

                ui.separator();
                ui.horizontal(|ui| {
                    let missing = self.missing.len();
                    if missing > 0 {
                        let response = ui.button(
                            RichText::new(format!("{missing} offline"))
                                .size(10.0)
                                .color(theme::DANGER),
                        );
                        if response.clicked() {
                            self.open_dialog(Dialog::Missing);
                        }
                    } else {
                        ui.label(
                            RichText::new("All media online")
                                .size(10.0)
                                .color(theme::GREEN),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(!assets.is_empty(), egui::Button::new("Add all"))
                            .clicked()
                        {
                            add_all = true;
                        }
                    });
                });

                if let Some((placement, id)) = placement {
                    self.add_media_to(&id, placement);
                }
                if let Some(id) = relink {
                    self.open_dialog(Dialog::Relink(id));
                }
                if add_all {
                    self.push_undo();
                    let mut added = 0usize;
                    for asset in self.project.media.clone() {
                        if self.missing.contains(&asset.id) {
                            continue;
                        }
                        if matches!(asset.kind, MediaKind::Video | MediaKind::Image)
                            && edit::add_asset_to_timeline(&mut self.project, &asset).is_ok()
                        {
                            added += 1;
                        }
                    }
                    if added > 0 {
                        self.mark_edited();
                        self.set_status(format!("Added {added} visual clips to Video 1"), false);
                    }
                }
            });
    }

    fn preview_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ctx, |ui| {
                let duration = self.duration();
                let (playhead, playing, texture, rendering, diagnostic) =
                    if let Some(preview) = &self.preview {
                        (
                            preview.playhead,
                            preview.playing,
                            preview.texture().cloned(),
                            preview.is_rendering(),
                            preview.diagnostic(),
                        )
                    } else {
                        (0.0, false, None, false, self.preview_error.clone())
                    };

                ui.horizontal(|ui| {
                    theme::section_label(ui, "Preview");
                    theme::badge(ui, "Proxy 960×540", theme::BLUE);
                    if !self.missing.is_empty() {
                        theme::badge(ui, "Offline placeholders", theme::DANGER);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                theme::format_time(playhead),
                                theme::format_time(duration)
                            ))
                            .monospace()
                            .size(12.0)
                            .color(theme::TEXT),
                        );
                        if rendering {
                            ui.spinner();
                            ui.label(
                                RichText::new("rendering preview")
                                    .size(10.0)
                                    .color(theme::MUTED),
                            );
                        }
                    });
                });
                ui.add_space(6.0);

                let available = ui.available_size();
                let transport_height = 46.0;
                let frame_height = (available.y - transport_height - 8.0).max(140.0);
                let (outer, _) =
                    ui.allocate_exact_size(Vec2::new(available.x, frame_height), Sense::hover());
                let painter = ui.painter_at(outer);
                painter.rect_filled(
                    outer,
                    egui::CornerRadius::same(8),
                    Color32::from_rgb(3, 5, 8),
                );
                painter.rect_stroke(
                    outer,
                    egui::CornerRadius::same(8),
                    Stroke::new(1.0_f32, theme::BORDER),
                    egui::StrokeKind::Inside,
                );
                let target = fit_rect(outer.shrink(8.0), 16.0 / 9.0);
                if let Some(texture) = &texture {
                    painter.image(
                        texture.id(),
                        target,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    let (title, color) = if let Some(error) = &diagnostic {
                        (format!("Preview unavailable\n{error}"), theme::DANGER)
                    } else if self.clip_count() == 0 {
                        (
                            "Your story starts here\nImport media, then add it to the timeline"
                                .to_string(),
                            theme::MUTED,
                        )
                    } else if self.tools.is_none() {
                        (
                            "ffmpeg is required for preview\nInstall it or add it to PATH"
                                .to_string(),
                            theme::DANGER,
                        )
                    } else {
                        ("Rendering preview…".to_string(), theme::MUTED)
                    };
                    painter.text(
                        target.center(),
                        egui::Align2::CENTER_CENTER,
                        title,
                        FontId::proportional(14.0),
                        color,
                    );
                }
                painter.text(
                    outer.left_top() + Vec2::new(12.0, 9.0),
                    egui::Align2::LEFT_TOP,
                    format!("TC {}", theme::format_time(playhead)),
                    FontId::monospace(10.0),
                    theme::TEXT.gamma_multiply(0.8),
                );
                painter.text(
                    outer.right_top() + Vec2::new(-12.0, 9.0),
                    egui::Align2::RIGHT_TOP,
                    "1920 × 1080 · 30 fps",
                    FontId::monospace(9.5),
                    theme::MUTED,
                );

                ui.add_space(7.0);
                let mut toggle_playback = false;
                let mut seek: Option<f64> = None;
                ui.horizontal(|ui| {
                    let label = if playing { "Pause" } else { "Play" };
                    let button = egui::Button::new(RichText::new(label).strong().color(theme::BG))
                        .fill(theme::ACCENT)
                        .min_size(Vec2::new(66.0, 28.0));
                    if ui.add(button).clicked() {
                        toggle_playback = true;
                    }
                    if ui.button("Start").clicked() {
                        seek = Some(0.0);
                    }
                    if ui.button("−1s").clicked() {
                        seek = Some((playhead - 1.0).max(0.0));
                    }
                    if ui.button("+1s").clicked() {
                        seek = Some((playhead + 1.0).min(duration.max(0.0)));
                    }
                    let mut time = playhead;
                    let slider = egui::Slider::new(&mut time, 0.0..=duration.max(0.01))
                        .show_value(false)
                        .clamping(egui::SliderClamping::Always);
                    if ui.add(slider).changed() {
                        seek = Some(time);
                    }
                    ui.label(
                        RichText::new(theme::format_time(playhead))
                            .monospace()
                            .size(11.0)
                            .color(theme::TEXT),
                    );
                });
                if let Some(error) = &diagnostic {
                    ui.label(RichText::new(error).size(10.0).color(theme::DANGER));
                }
                if toggle_playback {
                    if let Some(preview) = &mut self.preview {
                        preview.toggle_playback(duration);
                    }
                }
                if let Some(time) = seek {
                    self.seek_to(time);
                }
            });
    }

    fn inspector_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(304.0)
            .width_range(260.0..=392.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::section_label(ui, "Inspector");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.selected_clip.is_some() {
                            theme::badge(ui, "Clip", theme::ACCENT);
                        } else {
                            theme::badge(ui, "Project", theme::BLUE);
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .id_salt("inspector-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.inspector_contents(ui);
                    });
            });
    }

    fn inspector_contents(&mut self, ui: &mut Ui) {
        let Some(selection) = self.selected_clip.clone() else {
            self.project_inspector(ui);
            return;
        };
        let Some((track_kind, track_name, clip_index, is_primary)) =
            self.project.clip_by_id(&selection).map(|(track, _)| {
                let index = track
                    .clips
                    .iter()
                    .position(|clip| clip.id == selection)
                    .unwrap_or(0);
                (
                    track.kind,
                    track.name.clone(),
                    index,
                    track.kind == TrackKind::Video,
                )
            })
        else {
            self.selected_clip = None;
            self.project_inspector(ui);
            return;
        };
        let asset = self
            .project
            .clip_by_id(&selection)
            .and_then(|(_, clip)| clip.media_id.clone())
            .and_then(|id| self.project.media_by_id(&id).cloned());
        let snapshot = self.project.clone();
        let mut changed = false;
        let mut do_split = false;
        let mut do_duplicate = false;
        let mut do_delete = false;

        ui.horizontal(|ui| {
            let color = match track_kind {
                TrackKind::Video => theme::BLUE,
                TrackKind::Overlay => Color32::from_rgb(183, 140, 255),
                TrackKind::Audio => theme::GREEN,
            };
            theme::badge(ui, &track_name, color);
            ui.label(
                RichText::new(format!(
                    "clip {} of {}",
                    clip_index + 1,
                    self.project
                        .track_by_id(
                            self.project
                                .clip_by_id(&selection)
                                .map(|(track, _)| track.id.as_str())
                                .unwrap_or("")
                        )
                        .map(|track| track.clips.len())
                        .unwrap_or(0)
                ))
                .size(10.0)
                .color(theme::MUTED),
            );
        });
        ui.add_space(4.0);

        let clip = self
            .project
            .clip_by_id_mut(&selection)
            .map(|(_, clip)| clip)
            .expect("selected clip exists");

        let name_response = ui.add(
            egui::TextEdit::singleline(&mut clip.name)
                .desired_width(f32::INFINITY)
                .hint_text("Clip name"),
        );
        changed |= name_response.changed();

        let duration = clip.duration();
        let mut new_duration = duration;
        egui::Grid::new("clip-meta")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Start").size(10.0).color(theme::MUTED));
                if track_kind == TrackKind::Video {
                    ui.label(
                        RichText::new(theme::format_time(clip.start))
                            .monospace()
                            .size(11.0),
                    );
                } else {
                    let response = ui.add(
                        egui::DragValue::new(&mut clip.start)
                            .speed(0.05)
                            .range(0.0..=86_400.0)
                            .suffix(" s"),
                    );
                    changed |= response.changed();
                }
                ui.end_row();
                ui.label(RichText::new("Duration").size(10.0).color(theme::MUTED));
                let editable_duration = clip.title.is_some()
                    || asset
                        .as_ref()
                        .map(|asset| asset.kind == MediaKind::Image)
                        .unwrap_or(false);
                if editable_duration {
                    let response = ui.add(
                        egui::DragValue::new(&mut new_duration)
                            .speed(0.05)
                            .range(0.1..=3_600.0)
                            .suffix(" s"),
                    );
                    changed |= response.changed();
                } else {
                    ui.label(
                        RichText::new(theme::format_time(duration))
                            .monospace()
                            .size(11.0),
                    );
                }
                ui.end_row();
                if let Some(asset) = &asset {
                    ui.label(RichText::new("Source").size(10.0).color(theme::MUTED));
                    ui.label(
                        RichText::new(format!(
                            "{} · in {:.2}s / out {:.2}s",
                            asset.name, clip.in_point, clip.out_point
                        ))
                        .size(10.0)
                        .color(theme::MUTED),
                    );
                    ui.end_row();
                }
            });
        if changed && (new_duration - duration).abs() > 0.0001 {
            clip.out_point = clip.in_point + new_duration.max(0.1);
        }

        if let Some(asset) = &asset {
            if self.missing.contains(&asset.id) {
                ui.add_space(4.0);
                theme::badge(ui, "Media offline · export blocked", theme::DANGER);
                if ui.button("Relink source…").clicked() {
                    self.dialog = Dialog::Relink(asset.id.clone());
                }
            }
        }

        if let Some(title) = &mut clip.title {
            ui.add_space(6.0);
            ui.separator();
            title_editor(ui, title, &mut changed);
        } else if is_primary {
            ui.add_space(6.0);
            ui.separator();
            theme::section_label(ui, "Incoming transition");
            if clip_index == 0 {
                theme::muted(
                    ui,
                    "The first clip has no incoming edge. Add a transition to any later clip.",
                );
            } else {
                let current = clip
                    .transition
                    .as_ref()
                    .map(|transition| catalog::transition_name(transition.kind))
                    .unwrap_or("None");
                egui::ComboBox::from_id_salt("clip-transition")
                    .selected_text(current)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                if ui
                                    .selectable_label(clip.transition.is_none(), "None")
                                    .clicked()
                                {
                                    clip.transition = None;
                                    changed = true;
                                }
                                for (name, kind) in TRANSITIONS {
                                    let selected = clip
                                        .transition
                                        .as_ref()
                                        .map(|transition| transition.kind == *kind)
                                        .unwrap_or(false);
                                    if ui.selectable_label(selected, *name).clicked() {
                                        clip.transition = Some(Transition {
                                            kind: *kind,
                                            duration: clip
                                                .transition
                                                .as_ref()
                                                .map(|transition| transition.duration)
                                                .unwrap_or_else(
                                                    catalog::default_transition_duration,
                                                ),
                                        });
                                        changed = true;
                                    }
                                }
                            });
                    });
                if let Some(transition) = &mut clip.transition {
                    let response = ui.add(
                        egui::Slider::new(&mut transition.duration, 0.1..=2.0)
                            .text("Duration")
                            .suffix(" s"),
                    );
                    changed |= response.changed();
                }
            }

            ui.add_space(6.0);
            ui.separator();
            theme::section_label(ui, "Effects");
            let mut remove_effect = None;
            for (index, effect) in clip.effects.iter_mut().enumerate() {
                let name = catalog::effect_name(effect.kind);
                let def = catalog::effect_def(effect.kind);
                egui::Frame::new()
                    .fill(theme::PANEL_RAISED)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).size(12.0).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("×").clicked() {
                                    remove_effect = Some(index);
                                }
                            });
                        });
                        if let Some(def) = def {
                            for param in def.params {
                                let value = effect
                                    .params
                                    .entry(param.key.to_string())
                                    .or_insert(param.default);
                                let response = ui.add(
                                    egui::Slider::new(value, param.min..=param.max)
                                        .text(param.label)
                                        .step_by(param.step),
                                );
                                changed |= response.changed();
                            }
                        }
                    });
                ui.add_space(4.0);
            }
            if let Some(index) = remove_effect {
                clip.effects.remove(index);
                changed = true;
            }
            ui.menu_button("+ Add effect", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        for def in EFFECTS {
                            let response = ui.button(def.name).on_hover_text(def.description);
                            if response.clicked() {
                                if let Some(effect) = catalog::make_effect(def.kind) {
                                    clip.effects.push(effect);
                                    changed = true;
                                }
                                ui.close_menu();
                            }
                        }
                    });
            });
        }

        ui.add_space(6.0);
        ui.separator();
        theme::section_label(ui, "Audio");
        let has_audio = track_kind == TrackKind::Audio
            || asset.as_ref().map(|asset| asset.has_audio).unwrap_or(false);
        if has_audio {
            audio_editor(ui, &mut clip.audio, duration, &mut changed);
        } else {
            theme::muted(ui, "This clip has no audio stream.");
        }

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Split").clicked() {
                do_split = true;
            }
            if ui.button("Duplicate").clicked() {
                do_duplicate = true;
            }
            if ui.button("Delete").clicked() {
                do_delete = true;
            }
        });
        theme::muted(
            ui,
            "Timeline handles trim in/out; these actions edit the selected clip only.",
        );

        if changed {
            self.push_undo_snapshot(snapshot);
            if is_primary {
                edit::reflow_primary_ordered(&mut self.project);
            }
            self.mark_edited();
        }
        if do_split {
            self.split_selected();
        }
        if do_duplicate {
            self.duplicate_selected();
        }
        if do_delete {
            self.delete_selected();
        }
    }

    fn project_inspector(&mut self, ui: &mut Ui) {
        let snapshot = self.project.clone();
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Project").size(15.0).strong());
            if self.dirty {
                theme::badge(ui, "Unsaved", theme::ACCENT);
            }
        });
        ui.add_space(4.0);
        let name_response = ui.add(
            egui::TextEdit::singleline(&mut self.project.name)
                .desired_width(f32::INFINITY)
                .hint_text("Project name"),
        );
        changed |= name_response.changed();

        egui::Grid::new("project-meta")
            .num_columns(2)
            .spacing([12.0, 5.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Format").size(10.0).color(theme::MUTED));
                ui.label(RichText::new("splice-project v1").monospace().size(10.5));
                ui.end_row();
                ui.label(RichText::new("Canvas").size(10.0).color(theme::MUTED));
                ui.label(
                    RichText::new(format!(
                        "{} × {} · {} fps",
                        self.project.settings.width,
                        self.project.settings.height,
                        self.project.settings.fps
                    ))
                    .size(10.5),
                );
                ui.end_row();
                ui.label(RichText::new("Audio").size(10.0).color(theme::MUTED));
                ui.label(
                    RichText::new(format!("{} Hz · stereo", self.project.settings.sample_rate))
                        .size(10.5),
                );
                ui.end_row();
                ui.label(RichText::new("Duration").size(10.0).color(theme::MUTED));
                ui.label(
                    RichText::new(theme::format_time(self.project.timeline_duration()))
                        .monospace()
                        .size(10.5),
                );
                ui.end_row();
                ui.label(RichText::new("Media").size(10.0).color(theme::MUTED));
                ui.label(
                    RichText::new(format!(
                        "{} assets · {} offline",
                        self.project.media.len(),
                        self.missing.len()
                    ))
                    .size(10.5),
                );
                ui.end_row();
                ui.label(RichText::new("Clips").size(10.0).color(theme::MUTED));
                ui.label(RichText::new(self.clip_count().to_string()).size(10.5));
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.separator();
        theme::section_label(ui, "Tracks");
        for track in &mut self.project.tracks {
            ui.horizontal(|ui| {
                let response = ui.checkbox(&mut track.muted, "");
                changed |= response.changed();
                ui.label(RichText::new(&track.name).size(11.5));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    theme::muted(
                        ui,
                        format!(
                            "{} clip{}",
                            track.clips.len(),
                            if track.clips.len() == 1 { "" } else { "s" }
                        ),
                    );
                });
            });
        }

        ui.add_space(8.0);
        ui.separator();
        theme::section_label(ui, "Story tools");
        if ui.button("Add opening title…").clicked() {
            self.open_dialog(Dialog::Title);
        }
        if ui.button("AutoMovie…").clicked() {
            self.open_dialog(Dialog::AutoMovie);
        }
        if ui.button("Export movie…").clicked() {
            self.open_dialog(Dialog::Export);
        }
        if ui.button("Import media…").clicked() {
            self.open_dialog(Dialog::Import);
        }
        ui.add_space(6.0);
        theme::muted(
            ui,
            "Select a clip in the storyboard or timeline to edit transitions, effects, titles and audio.",
        );

        if changed {
            self.push_undo_snapshot(snapshot);
            self.dirty = true;
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let dialog = self.dialog.clone();
        match dialog {
            Dialog::None => {}
            Dialog::Open => self.dialog_open(ctx),
            Dialog::SaveAs => self.dialog_save_as(ctx),
            Dialog::Import => self.dialog_import(ctx),
            Dialog::Export => self.dialog_export(ctx),
            Dialog::AutoMovie => self.dialog_automovie(ctx),
            Dialog::Voiceover => self.dialog_voiceover(ctx),
            Dialog::Title => self.dialog_title(ctx),
            Dialog::Relink(id) => self.dialog_relink(ctx, &id),
            Dialog::Missing => self.dialog_missing(ctx),
            Dialog::About => self.dialog_about(ctx),
        }
    }

    fn dialog_open(&mut self, ctx: &egui::Context) {
        let mut window_open = true;
        egui::Window::new("Open project")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(470.0)
            .show(ctx, |ui| {
                theme::muted(
                    ui,
                    "Splice projects are UTF-8 JSON files with the .splice extension.",
                );
                ui.add_space(5.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.open_path)
                        .desired_width(f32::INFINITY)
                        .hint_text("/path/to/movie.splice"),
                );
                let mut open_clicked = false;
                if response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                    open_clicked = true;
                }
                if let Some(error) = &self.dialog_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        open_clicked = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_dialog();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::muted(ui, "You can also drop a .splice file on the window.");
                    });
                });
                if open_clicked {
                    let path = PathBuf::from(self.open_path.trim());
                    if path.as_os_str().is_empty() {
                        self.dialog_error = Some("Choose a project file".to_string());
                    } else {
                        self.open_project(&path);
                    }
                }
            });
        if !window_open {
            self.close_dialog();
        }
    }

    fn dialog_save_as(&mut self, ctx: &egui::Context) {
        let mut window_open = true;
        egui::Window::new("Save project")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(470.0)
            .show(ctx, |ui| {
                theme::muted(
                    ui,
                    "Saves atomically: a temp file in the same directory replaces the project only after a full write and fsync.",
                );
                ui.add_space(5.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.save_path)
                        .desired_width(f32::INFINITY)
                        .hint_text("/path/to/movie.splice"),
                );
                let mut save_clicked = false;
                if response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                    save_clicked = true;
                }
                if let Some(error) = &self.dialog_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save_clicked = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_dialog();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::muted(ui, "The .splice extension is added automatically.");
                    });
                });
                if save_clicked {
                    let path = PathBuf::from(self.save_path.trim());
                    if path.as_os_str().is_empty() {
                        self.dialog_error = Some("Choose a project path".to_string());
                    } else {
                        self.save_to(ctx, &path);
                    }
                }
            });
        if !window_open {
            self.close_dialog();
        }
    }

    fn dialog_import(&mut self, ctx: &egui::Context) {
        let mut window_open = true;
        egui::Window::new("Import media")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(500.0)
            .show(ctx, |ui| {
                theme::muted(
                    ui,
                    "Video, photos and audio are probed with the system ffprobe. Source files are referenced, never copied or rewritten.",
                );
                ui.add_space(5.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.import_path)
                        .desired_width(f32::INFINITY)
                        .hint_text("/path/to/clip.mp4, photo.jpg or music.wav"),
                );
                let mut import_clicked = false;
                if response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                    import_clicked = true;
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for kind in [
                        ("VIDEO", theme::BLUE),
                        ("IMAGE", theme::GOLD),
                        ("AUDIO", theme::GREEN),
                    ] {
                        theme::badge(ui, kind.0, kind.1);
                    }
                    theme::muted(ui, "One path per import.");
                });
                if self.tools.is_none() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("ffprobe is unavailable, so importing is disabled.")
                            .size(10.5)
                            .color(theme::DANGER),
                    );
                }
                if let Some(error) = &self.dialog_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.tools.is_some(), egui::Button::new("Import"))
                        .clicked()
                    {
                        import_clicked = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_dialog();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::muted(ui, "Drop files anywhere in the window to import.");
                    });
                });
                if import_clicked {
                    let path = PathBuf::from(self.import_path.trim());
                    if path.as_os_str().is_empty() {
                        self.dialog_error = Some("Choose a media file to import".to_string());
                    } else {
                        self.import_paths(vec![path]);
                    }
                }
            });
        if !window_open {
            self.close_dialog();
        }
    }

    fn dialog_export(&mut self, ctx: &egui::Context) {
        let mut window_open = true;
        let exporting = self.export_rx.is_some();
        egui::Window::new("Export movie")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(520.0)
            .show(ctx, |ui| {
                theme::section_label(ui, "Preset");
                let mut preset_changed = false;
                ui.horizontal_wrapped(|ui| {
                    for preset in ExportPreset::ALL {
                        if ui
                            .selectable_label(
                                self.export_preset == preset,
                                format!(
                                    "{}  {}×{}",
                                    preset.label(),
                                    preset.width(),
                                    preset.height()
                                ),
                            )
                            .clicked()
                        {
                            self.export_preset = preset;
                            preset_changed = true;
                        }
                    }
                });
                if preset_changed {
                    self.export_path = self.suggested_export_path();
                }
                ui.add_space(5.0);
                theme::section_label(ui, "Output file");
                ui.add(
                    egui::TextEdit::singleline(&mut self.export_path)
                        .desired_width(f32::INFINITY)
                        .hint_text("/path/to/movie.mp4"),
                );
                ui.add_space(5.0);
                ui.horizontal_wrapped(|ui| {
                    theme::badge(ui, "H.264 High", theme::BLUE);
                    theme::badge(ui, "AAC 192k", theme::GREEN);
                    theme::badge(ui, "48 kHz stereo", theme::GREEN);
                    theme::badge(ui, "30 fps", theme::GOLD);
                    theme::badge(ui, "+faststart", theme::MUTED);
                });
                ui.add_space(6.0);
                if !self.missing.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "Export is blocked: {} media asset(s) are offline.",
                            self.missing.len()
                        ))
                        .size(11.0)
                        .color(theme::DANGER),
                    );
                    for asset in self.missing_assets().iter().take(4) {
                        ui.label(
                            RichText::new(format!(
                                "  {} — {}",
                                asset.name,
                                resolve_media_path(&self.project_dir, asset).display()
                            ))
                            .size(9.5)
                            .color(theme::MUTED),
                        );
                    }
                } else {
                    theme::muted(
                        ui,
                        "Full project render with transitions, effects, titles, fades and the audio mix.",
                    );
                }
                if let Some(error) = &self.dialog_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                if exporting {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(format!(
                                "Encoding… {:.0}s",
                                self.export_started
                                    .map(|started| started.elapsed().as_secs_f32())
                                    .unwrap_or(0.0)
                            ))
                            .size(11.0)
                            .color(theme::TEXT),
                        );
                    });
                }
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    let can_export = !exporting
                        && self.tools.is_some()
                        && self.missing.is_empty()
                        && self.clip_count() > 0;
                    let export_button = egui::Button::new(
                        RichText::new("Export MP4").strong().color(theme::BG),
                    )
                    .fill(theme::ACCENT);
                    if ui
                        .add_enabled(can_export, export_button)
                        .clicked()
                    {
                        self.start_export();
                    }
                    if ui.button("Close").clicked() && !exporting {
                        self.close_dialog();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::muted(
                            ui,
                            "The same encoder is used by the CLI and the GUI.",
                        );
                    });
                });
            });
        if !window_open {
            self.close_dialog();
        }
    }

    fn dialog_missing(&mut self, ctx: &egui::Context) {
        let mut window_open = true;
        let missing: Vec<(String, String, String)> = self
            .missing_assets()
            .iter()
            .map(|asset| {
                (
                    asset.id.clone(),
                    asset.name.clone(),
                    resolve_media_path(&self.project_dir, asset)
                        .display()
                        .to_string(),
                )
            })
            .collect();
        let mut relink: Option<String> = None;
        egui::Window::new("Missing media")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "These sources are offline. Preview shows labelled placeholders; export is blocked until every asset is relinked.",
                    )
                    .size(11.0)
                    .color(theme::MUTED),
                );
                ui.add_space(7.0);
                for (id, name, path) in &missing {
                    egui::Frame::new()
                        .fill(theme::PANEL_RAISED)
                        .stroke(Stroke::new(1.0_f32, theme::DANGER.gamma_multiply(0.45)))
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(name).size(12.0).strong());
                                    ui.label(
                                        RichText::new(path).size(9.5).color(theme::MUTED),
                                    );
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("Relink…").clicked() {
                                        relink = Some(id.clone());
                                    }
                                });
                            });
                        });
                    ui.add_space(4.0);
                }
                if missing.is_empty() {
                    ui.label(RichText::new("All media is online.").color(theme::GREEN));
                }
                ui.add_space(4.0);
                if ui.button("Close").clicked() {
                    self.close_dialog();
                }
            });
        if !window_open {
            self.close_dialog();
        }
        if let Some(id) = relink {
            self.open_dialog(Dialog::Relink(id));
        }
    }

    fn dialog_relink(&mut self, ctx: &egui::Context, id: &str) {
        let Some(asset) = self.project.media_by_id(id).cloned() else {
            self.close_dialog();
            return;
        };
        let mut window_open = true;
        egui::Window::new("Relink media")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(&asset.name).size(14.0).strong());
                ui.label(
                    RichText::new(format!(
                        "Current path: {}",
                        resolve_media_path(&self.project_dir, &asset).display()
                    ))
                    .size(10.0)
                    .color(theme::MUTED),
                );
                ui.add_space(5.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.relink_path)
                        .desired_width(f32::INFINITY)
                        .hint_text("/new/path/to/source"),
                );
                theme::muted(
                    ui,
                    "The replacement is validated with ffprobe before the project is changed.",
                );
                if let Some(error) = &self.dialog_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                ui.add_space(6.0);
                let mut relink_clicked = false;
                ui.horizontal(|ui| {
                    if ui.button("Relink").clicked() {
                        relink_clicked = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_dialog();
                    }
                });
                if relink_clicked {
                    let Some(tools) = self.tools.clone() else {
                        self.dialog_error =
                            Some("ffprobe is required to validate a relink".to_string());
                        return;
                    };
                    let path = PathBuf::from(self.relink_path.trim());
                    if path.as_os_str().is_empty() {
                        self.dialog_error = Some("Choose the replacement source".to_string());
                        return;
                    }
                    self.push_undo();
                    match library::relink_media(
                        &mut self.project,
                        &self.project_dir,
                        &tools,
                        id,
                        &path,
                    ) {
                        Ok(info) => {
                            self.mark_edited();
                            self.set_status(
                                format!(
                                    "Relinked {} · {:?} · {:.2}s · {}×{}",
                                    asset.name,
                                    info.kind,
                                    info.duration,
                                    info.width.unwrap_or(0),
                                    info.height.unwrap_or(0)
                                ),
                                false,
                            );
                            self.dialog = Dialog::None;
                            self.dialog_error = None;
                        }
                        Err(error) => self.dialog_error = Some(format!("{error:#}")),
                    }
                }
            });
        if !window_open {
            self.close_dialog();
        }
    }

    fn dialog_about(&mut self, ctx: &egui::Context) {
        let mut window_open = true;
        egui::Window::new("About Splice")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Splice")
                        .size(26.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.label(
                    RichText::new("A native Linux timeline editor in Rust")
                        .size(11.0)
                        .color(theme::MUTED),
                );
                ui.add_space(6.0);
                ui.separator();
                theme::section_label(ui, "System toolchain");
                if let Some(tools) = &self.tools {
                    ui.label(
                        RichText::new(tools.version_summary())
                            .monospace()
                            .size(10.0)
                            .color(theme::TEXT),
                    );
                } else {
                    ui.label(
                        RichText::new(
                            self.tool_error
                                .clone()
                                .unwrap_or_else(|| "ffmpeg not detected".to_string()),
                        )
                        .monospace()
                        .size(10.0)
                        .color(theme::DANGER),
                    );
                }
                ui.add_space(6.0);
                theme::section_label(ui, "Project");
                ui.label(
                    RichText::new("format: splice-project · version: 1 · UTF-8 JSON")
                        .monospace()
                        .size(10.0),
                );
                ui.label(
                    RichText::new(format!(
                        "canvas: {}×{} @ {} fps · audio {} Hz stereo",
                        self.project.settings.width,
                        self.project.settings.height,
                        self.project.settings.fps,
                        self.project.settings.sample_rate
                    ))
                    .monospace()
                    .size(10.0),
                );
                ui.add_space(6.0);
                theme::section_label(ui, "Capture");
                ui.label(
                    RichText::new(
                        self.voice_device
                            .clone()
                            .unwrap_or_else(|| "cpal default input (PipeWire/ALSA)".to_string()),
                    )
                    .size(10.0)
                    .color(theme::MUTED),
                );
                ui.add_space(6.0);
                ui.separator();
                ui.label(
                    RichText::new(
                        "Fully offline: no network calls, telemetry, uploads or auto-update. Only the system ffmpeg/ffprobe binaries are used.",
                    )
                    .size(10.5)
                    .color(theme::MUTED),
                );
                ui.add_space(6.0);
                if ui.button("Close").clicked() {
                    self.close_dialog();
                }
            });
        if !window_open {
            self.close_dialog();
        }
    }

    fn start_export(&mut self) {
        if self.export_rx.is_some() {
            return;
        }
        let Some(tools) = self.tools.clone() else {
            self.dialog_error = Some("ffmpeg/ffprobe are required to export".to_string());
            return;
        };
        if !self.missing.is_empty() {
            self.dialog_error = Some(format!(
                "Export blocked: {} media asset(s) are offline",
                self.missing.len()
            ));
            return;
        }
        if self.clip_count() == 0 {
            self.dialog_error = Some("The timeline is empty".to_string());
            return;
        }
        let output = PathBuf::from(self.export_path.trim());
        if output.as_os_str().is_empty() {
            self.dialog_error = Some("Choose an output .mp4 path".to_string());
            return;
        }
        let project = self.project.clone();
        let project_dir = self.project_dir.clone();
        let preset = self.export_preset;
        let output_label = output.display().to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = export_project(&project, &project_dir, &tools, preset, &output)
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(result);
        });
        self.export_rx = Some(rx);
        self.export_started = Some(Instant::now());
        self.dialog_error = None;
        self.set_status(
            format!(
                "Exporting {} at {}…",
                output_label,
                self.export_preset.label()
            ),
            false,
        );
    }

    fn dialog_automovie(&mut self, ctx: &egui::Context) {
        let music_assets: Vec<(String, String)> = self
            .project
            .media
            .iter()
            .filter(|asset| asset.kind == MediaKind::Audio)
            .map(|asset| (asset.id.clone(), asset.name.clone()))
            .collect();
        if let Some(id) = &self.automovie.music_media_id {
            if !music_assets.iter().any(|(candidate, _)| candidate == id) {
                self.automovie.music_media_id = None;
            }
        }
        let mut window_open = true;
        egui::Window::new("AutoMovie")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(520.0)
            .show(ctx, |ui| {
                theme::muted(
                    ui,
                    "Builds a Movie Maker style sequence: opening title, crossfades between scenes, closing credits, and an optional music bed fitted to the movie.",
                );
                ui.add_space(6.0);
                theme::section_label(ui, "Opening");
                ui.checkbox(&mut self.automovie.opening_title, "Opening title");
                if self.automovie.opening_title {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.automovie.opening_text)
                            .desired_width(f32::INFINITY)
                            .hint_text("Project name"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.automovie.opening_duration, 0.5..=12.0)
                            .text("Duration")
                            .suffix(" s"),
                    );
                }
                ui.add_space(4.0);
                theme::section_label(ui, "Scene transitions");
                let current = catalog::transition_name(self.automovie.transition);
                egui::ComboBox::from_id_salt("automovie-transition")
                    .selected_text(current)
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for (name, kind) in TRANSITIONS {
                            ui.selectable_value(&mut self.automovie.transition, *kind, *name);
                        }
                    });
                ui.add(
                    egui::Slider::new(&mut self.automovie.transition_duration, 0.2..=2.0)
                        .text("Transition length")
                        .suffix(" s"),
                );
                ui.add_space(4.0);
                theme::section_label(ui, "Closing");
                ui.checkbox(&mut self.automovie.closing_credits, "Closing credits");
                if self.automovie.closing_credits {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.automovie.closing_text)
                            .desired_width(f32::INFINITY)
                            .hint_text("THE END"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.automovie.closing_subtitle)
                            .desired_width(f32::INFINITY)
                            .hint_text("Subtitle"),
                    );
                    ui.add(
                        egui::Slider::new(&mut self.automovie.closing_duration, 0.5..=16.0)
                            .text("Duration")
                            .suffix(" s"),
                    );
                }
                ui.add_space(4.0);
                theme::section_label(ui, "Music");
                ui.checkbox(
                    &mut self.automovie.fit_to_music,
                    "Fit the movie to the selected music",
                );
                let selected_music = self
                    .automovie
                    .music_media_id
                    .as_ref()
                    .and_then(|id| {
                        music_assets
                            .iter()
                            .find(|(candidate, _)| candidate == id)
                            .map(|(_, name)| name.clone())
                    })
                    .unwrap_or_else(|| "None".to_string());
                egui::ComboBox::from_id_salt("automovie-music")
                    .selected_text(selected_music)
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                self.automovie.music_media_id.is_none(),
                                "None",
                            )
                            .clicked()
                        {
                            self.automovie.music_media_id = None;
                        }
                        for (id, name) in &music_assets {
                            if ui
                                .selectable_label(
                                    self.automovie.music_media_id.as_deref() == Some(id.as_str()),
                                    name,
                                )
                                .clicked()
                            {
                                self.automovie.music_media_id = Some(id.clone());
                            }
                        }
                    });
                if music_assets.is_empty() {
                    theme::muted(ui, "Import audio into the library to use a music bed.");
                }
                if let Some(error) = &self.automovie_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.clip_count() > 0,
                            egui::Button::new(
                                RichText::new("Build AutoMovie")
                                    .strong()
                                    .color(theme::BG),
                            )
                            .fill(theme::ACCENT),
                        )
                        .clicked()
                    {
                        self.push_undo();
                        let options = self.automovie.clone();
                        match automovie::build(&mut self.project, &options) {
                            Ok(report) => {
                                self.selected_clip = self
                                    .project
                                    .primary_video_track()
                                    .and_then(|track| track.clips.first())
                                    .map(|clip| clip.id.clone());
                                self.mark_edited();
                                self.set_status(
                                    format!(
                                        "AutoMovie built · {} scenes · {:.1}s total · music: {}",
                                        report.clips,
                                        report.total_duration,
                                        report.music.unwrap_or_else(|| "none".to_string())
                                    ),
                                    false,
                                );
                                self.dialog = Dialog::None;
                                self.automovie_error = None;
                            }
                            Err(error) => {
                                self.automovie_error = Some(format!("{error:#}"));
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_dialog();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::muted(ui, "Running AutoMovie replaces generated titles.");
                    });
                });
            });
        if !window_open {
            self.close_dialog();
        }
    }

    fn dialog_title(&mut self, ctx: &egui::Context) {
        let mut window_open = true;
        let mut changed = false;
        egui::Window::new("Add title")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(520.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(520.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        title_editor(ui, &mut self.title_draft, &mut changed);
                        ui.add_space(4.0);
                        theme::section_label(ui, "Timing");
                        ui.add(
                            egui::Slider::new(&mut self.title_duration, 0.5..=30.0)
                                .text("Duration")
                                .suffix(" s"),
                        );
                        theme::muted(
                            ui,
                            format!(
                                "Placed at {} on the Overlay track.",
                                theme::format_time(self.playhead())
                            ),
                        );
                    });
                if let Some(error) = &self.title_error {
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Add title").strong().color(theme::BG))
                                .fill(theme::ACCENT),
                        )
                        .clicked()
                    {
                        if self.title_draft.text.trim().is_empty() {
                            self.title_error = Some("Title text cannot be empty".to_string());
                        } else {
                            self.add_title();
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_dialog();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::muted(ui, "Ten animated styles, editable after adding.");
                    });
                });
            });
        if !window_open {
            self.close_dialog();
        }
    }

    fn dialog_voiceover(&mut self, ctx: &egui::Context) {
        let recording = self.voice.is_some();
        let (elapsed, level, device) = if let Some(session) = &self.voice {
            (
                session.recorder.elapsed().as_secs_f64(),
                session.recorder.level_dbfs(),
                session.recorder.device_name().to_string(),
            )
        } else {
            (
                0.0,
                -60.0,
                self.voice_device
                    .clone()
                    .unwrap_or_else(|| "default input".to_string()),
            )
        };
        let stats = self.voice_stats.clone();
        let mut window_open = true;
        let mut start = false;
        let mut stop = false;
        egui::Window::new("Voiceover")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if recording {
                        theme::badge(ui, "Recording", theme::DANGER);
                    } else {
                        theme::badge(ui, "Ready", theme::GREEN);
                    }
                    ui.label(
                        RichText::new(&device)
                            .size(10.5)
                            .color(theme::MUTED),
                    );
                });
                ui.add_space(5.0);
                ui.label(
                    RichText::new(
                        "Captures the default PipeWire/ALSA input through cpal. The WAV is imported into the Voiceover track at the playhead when you stop.",
                    )
                    .size(10.5)
                    .color(theme::MUTED),
                );
                ui.add_space(7.0);
                if recording {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(format!("Recording {elapsed:.1}s"))
                                .size(13.0)
                                .strong()
                                .color(theme::TEXT),
                        );
                    });
                    ui.add_space(4.0);
                    let normalized = ((level + 60.0) / 60.0).clamp(0.0, 1.0);
                    ui.add_sized(
                        [440.0, 18.0],
                        egui::ProgressBar::new(normalized)
                            .text(format!("{level:.1} dBFS")),
                    );
                    if level < -50.0 {
                        ui.label(
                            RichText::new("Very low level — check the default microphone.")
                                .size(10.0)
                                .color(theme::ACCENT),
                        );
                    }
                    ui.add_space(6.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Stop & add to Voiceover")
                                    .strong()
                                    .color(theme::BG),
                            )
                            .fill(theme::ACCENT),
                        )
                        .clicked()
                    {
                        stop = true;
                    }
                } else {
                    if let Some(stats) = &stats {
                        theme::section_label(ui, "Last take");
                        ui.label(
                            RichText::new(format!(
                                "{} · {:.2}s · {} Hz · {} ch · RMS {:.1} dBFS · peak {:.1} dBFS",
                                stats.path.display(),
                                stats.seconds,
                                stats.sample_rate,
                                stats.channels,
                                stats.rms_dbfs,
                                stats.peak_dbfs
                            ))
                            .monospace()
                            .size(9.5)
                            .color(theme::MUTED),
                        );
                        if stats.non_silent() {
                            theme::badge(ui, "Non-silent recording verified", theme::GREEN);
                        } else {
                            theme::badge(ui, "Silent recording", theme::DANGER);
                        }
                    } else {
                        theme::muted(ui, "No voiceover recorded in this session yet.");
                    }
                    if let Some(error) = &self.voice_error {
                        ui.add_space(4.0);
                        ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                    }
                    ui.add_space(6.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Start recording").strong().color(theme::BG),
                            )
                            .fill(theme::ACCENT),
                        )
                        .clicked()
                    {
                        start = true;
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        self.close_dialog();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::muted(
                            ui,
                            format!("Insert point {}", theme::format_time(self.playhead())),
                        );
                    });
                });
            });
        if !window_open {
            self.close_dialog();
        }
        if start {
            self.start_voice_recording();
        }
        if stop {
            self.stop_voice_recording();
        }
        if self.voice.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn start_voice_recording(&mut self) {
        if self.voice.is_some() {
            return;
        }
        let directory = self.project_dir.join("voiceover");
        if let Err(error) = std::fs::create_dir_all(&directory) {
            self.voice_error = Some(format!("Could not create {}: {error}", directory.display()));
            return;
        }
        let path = directory.join(format!("{}.wav", new_id("voice")));
        match Recorder::start(&path) {
            Ok(recorder) => {
                self.voice = Some(VoiceSession {
                    recorder,
                    start_time: self.playhead(),
                });
                self.voice_error = None;
                self.voice_stats = None;
                self.set_status("Recording voiceover from the default input…", false);
            }
            Err(error) => {
                self.voice_error = Some(format!("{error:#}"));
                self.set_status(format!("Voiceover failed — {error:#}"), true);
            }
        }
    }

    fn stop_voice_recording(&mut self) {
        let Some(session) = self.voice.take() else {
            return;
        };
        let start_time = session.start_time;
        match session.recorder.stop() {
            Ok(stats) => {
                self.voice_stats = Some(stats.clone());
                if !stats.non_silent() {
                    self.voice_error = Some(format!(
                        "Recording looks silent (RMS {:.1} dBFS, peak {:.1} dBFS). Check the default microphone and try again.",
                        stats.rms_dbfs, stats.peak_dbfs
                    ));
                    self.set_status("Voiceover recording is silent", true);
                    return;
                }
                let Some(tools) = self.tools.clone() else {
                    self.voice_error =
                        Some("ffprobe is required to import the recording".to_string());
                    return;
                };
                let mut draft = self.project.clone();
                match library::import_media(
                    &mut draft,
                    Some(self.project_dir.as_path()),
                    &tools,
                    &stats.path,
                ) {
                    Ok(asset) => {
                        let asset_id = asset.id.clone();
                        self.push_undo();
                        self.project = draft;
                        if let Some(asset) = self.project.media_by_id(&asset_id).cloned() {
                            match edit::add_asset_to_voiceover(
                                &mut self.project,
                                &asset,
                                start_time,
                            ) {
                                Ok(clip_id) => self.selected_clip = Some(clip_id),
                                Err(error) => self.voice_error = Some(format!("{error:#}")),
                            }
                        }
                        self.mark_edited();
                        self.voice_error = None;
                        self.set_status(
                            format!(
                                "Voiceover recorded · {:.1}s · RMS {:.1} dBFS · peak {:.1} dBFS · non-silent",
                                stats.seconds, stats.rms_dbfs, stats.peak_dbfs
                            ),
                            false,
                        );
                    }
                    Err(error) => {
                        self.voice_error = Some(format!(
                            "Recording saved to {} but could not be imported: {error:#}",
                            stats.path.display()
                        ));
                        self.set_status("Voiceover import failed", true);
                    }
                }
            }
            Err(error) => {
                self.voice_error = Some(format!("{error:#}"));
                self.set_status(format!("Voiceover failed — {error:#}"), true);
            }
        }
    }
}

fn title_editor(ui: &mut Ui, title: &mut Title, changed: &mut bool) {
    theme::section_label(ui, "Title");
    let response = ui.add(
        egui::TextEdit::multiline(&mut title.text)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text("Title text"),
    );
    *changed |= response.changed();
    let response = ui.add(
        egui::TextEdit::singleline(&mut title.subtitle)
            .desired_width(f32::INFINITY)
            .hint_text("Subtitle (optional)"),
    );
    *changed |= response.changed();

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Style").size(10.0).color(theme::MUTED));
        egui::ComboBox::from_id_salt(ui.id().with("title-style"))
            .selected_text(catalog::title_style_name(title.style))
            .width(180.0)
            .show_ui(ui, |ui| {
                for (name, style) in TITLE_STYLES {
                    if ui.selectable_label(title.style == *style, *name).clicked() {
                        title.style = *style;
                        *changed = true;
                    }
                }
            });
    });
    ui.label(
        RichText::new(catalog::title_style_description(title.style))
            .size(9.5)
            .color(theme::MUTED),
    );

    ui.horizontal(|ui| {
        ui.label(RichText::new("Font").size(10.0).color(theme::MUTED));
        let mut font = title.font.clone();
        egui::ComboBox::from_id_salt(ui.id().with("title-font"))
            .selected_text(font.clone())
            .width(180.0)
            .show_ui(ui, |ui| {
                for candidate in FONTS {
                    if ui
                        .selectable_label(font == *candidate, *candidate)
                        .clicked()
                    {
                        font = (*candidate).to_string();
                    }
                }
            });
        if font != title.font {
            title.font = font;
            *changed = true;
        }
    });

    let response = ui.add(
        egui::Slider::new(&mut title.size, 12.0..=180.0)
            .text("Size")
            .step_by(1.0),
    );
    *changed |= response.changed();

    color_row(ui, "Text", &mut title.color, changed);
    color_row(ui, "Background", &mut title.background, changed);
    let response = ui.add(
        egui::Slider::new(&mut title.background_opacity, 0.0..=1.0).text("Background opacity"),
    );
    *changed |= response.changed();

    ui.horizontal(|ui| {
        let response = ui.add(egui::Slider::new(&mut title.position_x, 0.0..=1.0).text("X"));
        *changed |= response.changed();
        let response = ui.add(egui::Slider::new(&mut title.position_y, 0.0..=1.0).text("Y"));
        *changed |= response.changed();
    });
    let response = ui.checkbox(&mut title.bold, "Bold");
    *changed |= response.changed();
}

fn color_row(ui: &mut Ui, label: &str, value: &mut String, changed: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.0).color(theme::MUTED));
        let mut color = parse_hex_color(value);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *value = hex_color(color);
            *changed = true;
        }
        ui.label(
            RichText::new(value.clone())
                .monospace()
                .size(9.0)
                .color(theme::MUTED),
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for swatch in PALETTE {
            let color = parse_hex_color(swatch);
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(3), color);
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(3),
                Stroke::new(
                    if value.eq_ignore_ascii_case(swatch) {
                        1.5_f32
                    } else {
                        1.0_f32
                    },
                    if value.eq_ignore_ascii_case(swatch) {
                        theme::ACCENT
                    } else {
                        theme::BORDER
                    },
                ),
                egui::StrokeKind::Inside,
            );
            if response.clicked() {
                *value = (*swatch).to_string();
                *changed = true;
            }
            response.on_hover_text(*swatch);
        }
    });
}

fn audio_editor(ui: &mut Ui, audio: &mut AudioParams, duration: f64, changed: &mut bool) {
    let response = ui.add(
        egui::Slider::new(&mut audio.volume, 0.0..=2.0)
            .text("Volume")
            .step_by(0.01),
    );
    *changed |= response.changed();
    let max_fade = duration.max(0.1) as f32;
    let response = ui.add(
        egui::Slider::new(&mut audio.fade_in, 0.0..=max_fade as f64)
            .text("Fade in")
            .suffix(" s"),
    );
    *changed |= response.changed();
    let response = ui.add(
        egui::Slider::new(&mut audio.fade_out, 0.0..=max_fade as f64)
            .text("Fade out")
            .suffix(" s"),
    );
    *changed |= response.changed();
    let response = ui.checkbox(&mut audio.muted, "Mute this clip");
    *changed |= response.changed();
}

fn fit_rect(bounds: Rect, aspect: f32) -> Rect {
    let mut width = bounds.width();
    let mut height = width / aspect;
    if height > bounds.height() {
        height = bounds.height();
        width = height * aspect;
    }
    Rect::from_center_size(bounds.center(), Vec2::new(width.max(1.0), height.max(1.0)))
}
