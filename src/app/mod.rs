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
use crate::autosave;
use crate::catalog::{self, EFFECTS, FONTS, PALETTE, TITLE_MOTIONS, TITLE_STYLES, TRANSITIONS};
use crate::edit;
use crate::export::{ExportReport, export_project_with};
use crate::ffmpeg::Toolchain;
use crate::filedialog;
use crate::history::History;
use crate::library;
use crate::model::{
    AudioParams, ClipMotion, Framing, MediaAsset, MediaKind, MotionDirection, MotionEasing,
    Project, Settings, Title, TitleMotion, TitleMotionPreset, TrackKind, Transition, new_id,
    resolve_media_path, stored_media_path,
};
use crate::presets::ExportPreset;
use crate::preview::{Preview, preview_dimensions};
use crate::project_io::{load_project, save_project};
use crate::runner::{Cancelled, RenderControl};
use crate::theme;
use crate::timeline_ui::{self, TimelineAction, TimelineCommand, TimelineUiState, TimelineView};
use crate::titles;

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
    /// Asks what to do with unsaved changes before running the action.
    ConfirmDiscard(PendingAction),
    /// An autosave newer than the project file was found.
    Recover(PathBuf),
}

/// A project-replacing action held back while the user decides about unsaved
/// changes.
/// Which dialog field a native file chooser fills in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickTarget {
    Open,
    SaveAs,
    Import,
    Export,
    Relink,
}

type PickOutcome = std::result::Result<Vec<PathBuf>, String>;

#[derive(Debug, Clone, PartialEq)]
enum PendingAction {
    NewProject,
    NewShort,
    Open(PathBuf),
    Quit,
}

struct VoiceSession {
    recorder: Recorder,
    start_time: f64,
}

/// Result of probing a batch of dropped or chosen files on a worker thread.
#[derive(Default)]
struct ImportBatch {
    assets: Vec<MediaAsset>,
    errors: Vec<String>,
}

enum ExportFailure {
    Cancelled,
    Failed(String),
}

type ExportOutcome = std::result::Result<ExportReport, ExportFailure>;

pub struct SpliceApp {
    project: Project,
    project_path: Option<PathBuf>,
    project_dir: PathBuf,
    tools: Option<Toolchain>,
    tool_error: Option<String>,

    preview: Option<Preview>,
    preview_error: Option<String>,
    shorts_safe_area: bool,
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
    effect_filter: String,
    transition_filter: String,

    export_preset: ExportPreset,
    export_path: String,
    export_rx: Option<Receiver<ExportOutcome>>,
    export_started: Option<Instant>,
    export_control: Option<RenderControl>,
    export_total: f64,
    import_rx: Option<Receiver<ImportBatch>>,
    picker: Option<filedialog::Backend>,
    pick_rx: Option<(Receiver<PickOutcome>, PickTarget)>,
    last_autosave: Instant,
    /// An autosave from a previous session is waiting for a decision, so new
    /// autosaves must not overwrite it.
    recovery_unresolved: bool,
    after_save: Option<PendingAction>,
    close_confirmed: bool,

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
        Self::from_options(options)
    }

    /// Builds the application state without a window, so tests can drive it
    /// with a plain `egui::Context`.
    pub fn from_options(options: AppOptions) -> Result<Self> {
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
        let export_preset = ExportPreset::matching(&project.settings);

        let mut app = Self {
            project,
            project_path,
            project_dir,
            tools,
            tool_error,
            preview: None,
            preview_error: None,
            shorts_safe_area: true,
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
            effect_filter: String::new(),
            transition_filter: String::new(),
            export_preset,
            export_path: String::new(),
            export_rx: None,
            export_started: None,
            export_control: None,
            export_total: 0.0,
            import_rx: None,
            picker: filedialog::detect(),
            pick_rx: None,
            last_autosave: Instant::now(),
            recovery_unresolved: false,
            after_save: None,
            close_confirmed: false,
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
        app.title_draft = Title::opening_for(&app.project.name, &app.project.settings);

        app.check_recovery();
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

        let capability_warnings = app
            .tools
            .as_ref()
            .map(Toolchain::capability_warnings)
            .unwrap_or_default();
        app.status = match &app.tools {
            Some(_) if capability_warnings.is_empty() => format!(
                "Ready — {} media · {} clips · ffmpeg detected",
                app.project.media.len(),
                app.clip_count()
            ),
            Some(_) => format!(
                "Ready — {} media · {} clips · missing: {}",
                app.project.media.len(),
                app.clip_count(),
                capability_warnings.join("; ")
            ),
            None => "ffmpeg/ffprobe were not found on PATH; preview and export are unavailable"
                .to_string(),
        };
        app.status_error = app.tools.is_none() || !capability_warnings.is_empty();
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
                self.title_draft = Title::opening_for(&self.project.name, &self.project.settings);
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
        self.after_save = None;
    }

    /// Runs `action`, first asking about unsaved changes when there are any.
    fn request(&mut self, ctx: &egui::Context, action: PendingAction) {
        if self.dirty {
            self.open_dialog(Dialog::ConfirmDiscard(action));
        } else {
            self.perform(ctx, action);
        }
    }

    /// Offers to restore an autosave that is newer than the project file.
    fn check_recovery(&mut self) {
        if let Some(recovery) = self
            .project_path
            .as_deref()
            .and_then(autosave::pending_recovery)
        {
            self.dialog = Dialog::Recover(recovery);
            self.recovery_unresolved = true;
        } else {
            self.recovery_unresolved = false;
        }
    }

    fn clear_autosave(&mut self) {
        if let Some(path) = &self.project_path {
            autosave::clear(path);
        }
    }

    /// Writes the working copy of a saved, edited project every few seconds.
    /// Unsaved new projects have no folder to put it in and are skipped.
    fn autosave_tick(&mut self) {
        if !self.dirty
            || self.recovery_unresolved
            || self.export_rx.is_some()
            || self.last_autosave.elapsed() < Duration::from_secs(autosave::AUTOSAVE_INTERVAL_SECS)
        {
            return;
        }
        self.last_autosave = Instant::now();
        if let Some(path) = self.project_path.clone() {
            if let Err(error) = autosave::write_autosave(&path, &self.project, &self.project_dir) {
                self.set_status(format!("Autosave failed — {error:#}"), true);
            }
        }
    }

    fn recover_from(&mut self, recovery: &Path) {
        match load_project(recovery) {
            Ok(loaded) => {
                self.push_undo();
                self.project = loaded.project;
                self.selected_clip = None;
                self.dialog = Dialog::None;
                self.recovery_unresolved = false;
                self.mark_edited();
                self.set_status("Recovered unsaved edits from the autosave", false);
            }
            Err(error) => {
                self.dialog_error = Some(format!("{error:#}"));
            }
        }
    }

    fn perform(&mut self, ctx: &egui::Context, action: PendingAction) {
        // Moving on from this project: its autosave is no longer wanted.
        self.clear_autosave();
        match action {
            PendingAction::NewProject => self.new_project(),
            PendingAction::NewShort => self.new_short_project(),
            PendingAction::Open(path) => self.open_project(&path),
            PendingAction::Quit => {
                self.close_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
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
                autosave::clear(&absolute);
                self.recovery_unresolved = false;
                self.refresh_missing();
                self.set_status(format!("Saved project · {}", absolute.display()), false);
                self.dialog = Dialog::None;
                self.dialog_error = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title("Splice".to_string()));
                if let Some(action) = self.after_save.take() {
                    self.perform(ctx, action);
                }
            }
            Err(error) => {
                self.dialog_error = Some(format!("{error:#}"));
            }
        }
    }

    fn open_project(&mut self, path: &Path) {
        match load_project(path) {
            Ok(loaded) => {
                self.import_rx = None;
                self.project = loaded.project;
                self.export_preset = ExportPreset::matching(&self.project.settings);
                self.title_draft = Title::opening_for(&self.project.name, &self.project.settings);
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
                self.check_recovery();
            }
            Err(error) => {
                self.dialog_error = Some(format!("{error:#}"));
                self.set_status(format!("Could not open project — {error:#}"), true);
            }
        }
    }

    fn new_project(&mut self) {
        self.reset_project(Project::new("Untitled"), "New project");
    }

    fn new_short_project(&mut self) {
        self.reset_project(Project::new_shorts("Untitled Short"), "New Short project");
    }

    fn reset_project(&mut self, project: Project, status: &str) {
        self.import_rx = None;
        self.project = project;
        self.project_path = None;
        self.project_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        self.selected_clip = None;
        self.selected_media = None;
        self.history.clear();
        self.dirty = false;
        self.drag_checkpoint = None;
        self.export_preset = ExportPreset::matching(&self.project.settings);
        self.save_path = self.suggested_save_path();
        self.import_path = self.project_dir.display().to_string();
        self.refresh_missing();
        self.invalidate_preview();
        self.title_draft = Title::opening_for(&self.project.name, &self.project.settings);
        self.set_status(status, false);
        self.dialog = Dialog::None;
    }

    fn import_paths(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if let Some(splice) = paths.iter().find(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("splice"))
                .unwrap_or(false)
        }) {
            let splice = splice.clone();
            self.request(ctx, PendingAction::Open(splice));
            return;
        }
        let Some(tools) = self.tools.clone() else {
            self.set_status("ffprobe is required to import media", true);
            return;
        };
        if self.import_rx.is_some() {
            self.set_status("An import is already running — wait for it to finish", true);
            return;
        }
        // Probing shells out to ffprobe, which can be slow on large or remote
        // files, so it runs off the UI thread.
        let count = paths.len();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut batch = ImportBatch::default();
            for path in paths {
                match library::probe_asset(None, &tools, &path) {
                    Ok(asset) => batch.assets.push(asset),
                    Err(error) => batch.errors.push(format!("{}: {error:#}", path.display())),
                }
            }
            let _ = tx.send(batch);
        });
        self.import_rx = Some(rx);
        self.set_status(format!("Importing {count} file(s)…"), false);
    }

    fn start_pick(&mut self, target: PickTarget) {
        let Some(backend) = self.picker else {
            return;
        };
        if self.pick_rx.is_some() {
            return;
        }
        let project_filter = (
            "Splice projects".to_string(),
            vec![crate::model::PROJECT_EXTENSION.to_string()],
        );
        let media_filter = (
            "Media".to_string(),
            [
                "mp4", "mov", "mkv", "webm", "avi", "m4v", "png", "jpg", "jpeg", "webp", "wav",
                "mp3", "flac", "ogg", "m4a", "aac",
            ]
            .iter()
            .map(|ext| ext.to_string())
            .collect(),
        );
        let movie_filter = ("MP4 movie".to_string(), vec!["mp4".to_string()]);
        let existing = |text: &str| {
            let path = PathBuf::from(text.trim());
            if path.as_os_str().is_empty() {
                self.project_dir.clone()
            } else {
                path
            }
        };
        let request = match target {
            PickTarget::Open => filedialog::Request {
                mode: filedialog::Mode::Open,
                title: "Open project".into(),
                start: existing(&self.open_path),
                filters: vec![project_filter],
            },
            PickTarget::SaveAs => filedialog::Request {
                mode: filedialog::Mode::Save,
                title: "Save project".into(),
                start: existing(&self.save_path),
                filters: vec![project_filter],
            },
            PickTarget::Import => filedialog::Request {
                mode: filedialog::Mode::OpenMany,
                title: "Import media".into(),
                start: existing(&self.import_path),
                filters: vec![media_filter],
            },
            PickTarget::Export => filedialog::Request {
                mode: filedialog::Mode::Save,
                title: "Export movie".into(),
                start: existing(&self.export_path),
                filters: vec![movie_filter],
            },
            PickTarget::Relink => filedialog::Request {
                mode: filedialog::Mode::Open,
                title: "Relink media".into(),
                start: existing(&self.relink_path),
                filters: vec![media_filter],
            },
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(filedialog::pick(backend, &request).map_err(|e| format!("{e:#}")));
        });
        self.pick_rx = Some((rx, target));
    }

    fn poll_pick(&mut self, ctx: &egui::Context) {
        let Some((rx, target)) = &self.pick_rx else {
            return;
        };
        let target = *target;
        let outcome = match rx.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("file chooser stopped unexpectedly".into()),
        };
        self.pick_rx = None;
        let paths = match outcome {
            Ok(paths) if paths.is_empty() => return, // cancelled
            Ok(paths) => paths,
            Err(error) => {
                self.dialog_error = Some(error);
                return;
            }
        };
        let first = paths[0].display().to_string();
        match target {
            PickTarget::Open => self.open_path = first,
            PickTarget::SaveAs => self.save_path = first,
            PickTarget::Export => self.export_path = first,
            PickTarget::Relink => self.relink_path = first,
            PickTarget::Import => self.import_paths(ctx, paths),
        }
    }

    fn poll_import(&mut self) {
        let Some(rx) = &self.import_rx else {
            return;
        };
        let batch = match rx.try_recv() {
            Ok(batch) => batch,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => ImportBatch {
                assets: Vec::new(),
                errors: vec!["import worker stopped unexpectedly".to_string()],
            },
        };
        self.import_rx = None;

        let imported = batch.assets.len();
        if imported > 0 {
            self.push_undo();
            for mut asset in batch.assets {
                // Stored relative to the project folder as it is now, which
                // may differ from where the import started.
                asset.path = stored_media_path(&self.project_dir, Path::new(&asset.path));
                self.selected_media = Some(asset.id.clone());
                self.project.media.push(asset);
            }
            self.mark_edited();
            self.set_status(
                format!(
                    "Imported {imported} file(s) · {} media in library",
                    self.project.media.len()
                ),
                false,
            );
        }
        if !batch.errors.is_empty() {
            self.set_status(
                format!("Import failed — {}", batch.errors.join(" · ")),
                true,
            );
            if self.dialog == Dialog::Import {
                self.dialog_error = Some(batch.errors.join("\n"));
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
        let snapshot = self.project.clone();
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
                self.push_undo_snapshot(snapshot);
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
            Err(error) => {
                // A failed add may have partly changed the project.
                self.project = snapshot;
                self.set_status(format!("Could not add clip: {error:#}"), true);
            }
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
        let snapshot = self.project.clone();
        if edit::delete_clip(&mut self.project, &id) {
            self.push_undo_snapshot(snapshot);
            self.selected_clip = None;
            self.mark_edited();
            self.set_status("Clip deleted", false);
        }
    }

    fn duplicate_selected(&mut self) {
        let Some(id) = self.selected_clip.clone() else {
            return;
        };
        let snapshot = self.project.clone();
        match edit::duplicate_clip(&mut self.project, &id) {
            Ok(new_id) => {
                self.push_undo_snapshot(snapshot);
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
        let snapshot = self.project.clone();
        match edit::split_clip(&mut self.project, &id, at) {
            Ok(new_id) => {
                self.push_undo_snapshot(snapshot);
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
                    let old_in = clip.in_point;
                    let old_duration = clip.duration();
                    clip.start = start.max(0.0);
                    clip.in_point = in_point.max(0.0);
                    clip.out_point = out_point.max(clip.in_point + 0.05);
                    let new_duration = clip.duration();
                    let delta = clip.in_point - old_in;
                    if delta.abs() > 0.0001 {
                        clip.motion = clip.motion.trim_start(delta, new_duration);
                    } else if (new_duration - old_duration).abs() > 0.0001 {
                        clip.motion = clip.motion.trim_end(new_duration);
                    }
                }
                self.dirty = true;
                self.invalidate_preview();
            }
            TimelineAction::MoveKeyframe {
                clip_id,
                index,
                time,
            } => {
                if self.drag_checkpoint.as_deref() != Some(clip_id.as_str()) {
                    self.push_undo();
                    self.drag_checkpoint = Some(clip_id.clone());
                }
                if let Some((_, clip)) = self.project.clip_by_id_mut(&clip_id) {
                    let clip_duration = clip.duration();
                    let origin = clip.motion.origin;
                    if let Some(keyframe) = clip.motion.keyframes.get_mut(index) {
                        keyframe.time = origin + time.clamp(0.0, clip_duration);
                    }
                }
                self.dirty = true;
                self.invalidate_preview();
            }
            TimelineAction::FinishKeyframe(clip_id) => {
                self.drag_checkpoint = None;
                self.history.break_coalescing();
                if let Some((_, clip)) = self.project.clip_by_id_mut(&clip_id) {
                    clip.motion.sort_and_dedupe();
                }
                self.mark_edited();
            }
            TimelineAction::DeleteKeyframe { clip_id, index } => {
                self.push_undo();
                if let Some((_, clip)) = self.project.clip_by_id_mut(&clip_id) {
                    if index < clip.motion.keyframes.len() {
                        clip.motion.keyframes.remove(index);
                    }
                }
                self.mark_edited();
                self.set_status("Motion keyframe deleted", false);
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
                let snapshot = self.project.clone();
                match edit::split_clip(&mut self.project, &id, at) {
                    Ok(new_id) => {
                        self.push_undo_snapshot(snapshot);
                        self.selected_clip = new_id;
                        self.mark_edited();
                    }
                    Err(error) => self.set_status(format!("{error:#}"), true),
                }
            }
            TimelineCommand::Duplicate(id) => {
                let snapshot = self.project.clone();
                match edit::duplicate_clip(&mut self.project, &id) {
                    Ok(new_id) => {
                        self.push_undo_snapshot(snapshot);
                        self.selected_clip = new_id;
                        self.mark_edited();
                    }
                    Err(error) => self.set_status(format!("{error:#}"), true),
                }
            }
            TimelineCommand::Delete(id) => {
                let snapshot = self.project.clone();
                if edit::delete_clip(&mut self.project, &id) {
                    self.push_undo_snapshot(snapshot);
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
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Stop a running export so ffmpeg is killed and its temporary file is
        // removed instead of being orphaned when the window closes.
        if let (Some(control), Some(rx)) = (&self.export_control, &self.export_rx) {
            control.cancel();
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
        if let Some(session) = self.voice.take() {
            let _ = session.recorder.stop();
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.run_frame(ctx);
    }
}

impl SpliceApp {
    /// One frame of the application: input, background work, then every panel.
    pub fn run_frame(&mut self, ctx: &egui::Context) {
        self.frame_count += 1;
        if ctx.input(|input| input.viewport().close_requested())
            && self.dirty
            && !self.close_confirmed
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !matches!(self.dialog, Dialog::ConfirmDiscard(_)) {
                self.open_dialog(Dialog::ConfirmDiscard(PendingAction::Quit));
            }
        }
        self.handle_events(ctx);
        self.poll_export();
        self.poll_import();
        self.poll_pick(ctx);
        self.autosave_tick();
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
            self.import_paths(ctx, dropped);
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
            self.request(ctx, PendingAction::NewProject);
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
                    outcome = Some(Err(ExportFailure::Failed(
                        "export worker stopped unexpectedly".to_string(),
                    )));
                    clear = true;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if clear {
            self.export_rx = None;
            self.export_started = None;
            self.export_control = None;
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
                Err(ExportFailure::Cancelled) => {
                    self.dialog_error = None;
                    self.set_status("Export cancelled", false);
                }
                Err(ExportFailure::Failed(error)) => {
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
        needs_repaint |= self.export_rx.is_some() || self.import_rx.is_some();
        needs_repaint |= self.pick_rx.is_some();
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
}

mod dialogs;
mod editors;
mod panels;

#[cfg(test)]
mod tests;
