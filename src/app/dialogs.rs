use super::editors::*;
use super::*;

impl SpliceApp {
    /// A "Browse…" button for the field above, shown only when a native file
    /// chooser (zenity or kdialog) is installed.
    pub(super) fn browse_row(&mut self, ui: &mut Ui, target: PickTarget) {
        if self.picker.is_none() {
            return;
        }
        let busy = self.pick_rx.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Browse…"))
                .clicked()
            {
                self.start_pick(target);
            }
            if busy {
                ui.spinner();
            }
        });
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
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
            Dialog::ConfirmDiscard(action) => self.dialog_confirm_discard(ctx, action),
            Dialog::Recover(path) => self.dialog_recover(ctx, path),
        }
    }

    pub(super) fn dialog_open(&mut self, ctx: &egui::Context) {
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
                self.browse_row(ui, PickTarget::Open);
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
                        self.request(ctx, PendingAction::Open(path));
                    }
                }
            });
        if !window_open {
            self.close_dialog();
        }
    }

    pub(super) fn dialog_save_as(&mut self, ctx: &egui::Context) {
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
                self.browse_row(ui, PickTarget::SaveAs);
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

    pub(super) fn dialog_import(&mut self, ctx: &egui::Context) {
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
                self.browse_row(ui, PickTarget::Import);
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
                        self.import_paths(ctx, vec![path]);
                    }
                }
            });
        if !window_open {
            self.close_dialog();
        }
    }

    pub(super) fn dialog_export(&mut self, ctx: &egui::Context) {
        let mut window_open = true;
        let exporting = self.export_rx.is_some();
        egui::Window::new("Export movie")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(520.0)
            .show(ctx, |ui| {
                let mut preset_changed = false;
                for (heading, vertical) in [("Landscape 16:9", false), ("Shorts 9:16", true)] {
                    theme::section_label(ui, heading);
                    ui.horizontal_wrapped(|ui| {
                        for preset in ExportPreset::ALL
                            .into_iter()
                            .filter(|preset| preset.is_vertical() == vertical)
                        {
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
                    ui.add_space(4.0);
                }
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
                self.browse_row(ui, PickTarget::Export);
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
                    let fraction = self
                        .export_control
                        .as_ref()
                        .map(|control| {
                            (control.progress_seconds() / self.export_total.max(0.1))
                                .clamp(0.0, 1.0)
                        })
                        .unwrap_or(0.0);
                    let elapsed = self
                        .export_started
                        .map(|started| started.elapsed().as_secs_f32())
                        .unwrap_or(0.0);
                    ui.add(
                        egui::ProgressBar::new(fraction as f32)
                            .text(format!("Encoding… {:.0}%  ·  {elapsed:.0}s", fraction * 100.0))
                            .desired_width(f32::INFINITY),
                    );
                    if self
                        .export_control
                        .as_ref()
                        .is_some_and(RenderControl::is_cancelled)
                    {
                        theme::muted(ui, "Cancelling…");
                    }
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
                    if exporting {
                        if ui.button("Cancel export").clicked() {
                            if let Some(control) = &self.export_control {
                                control.cancel();
                            }
                        }
                    } else if ui.button("Close").clicked() {
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

    pub(super) fn dialog_confirm_discard(&mut self, ctx: &egui::Context, action: PendingAction) {
        let mut window_open = true;
        let what = match &action {
            PendingAction::NewProject | PendingAction::NewShort => "start a new project",
            PendingAction::Open(_) => "open another project",
            PendingAction::Quit => "quit",
        };
        egui::Window::new("Unsaved changes")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "“{}” has unsaved changes. Save them before you {what}?",
                        self.project.name
                    ))
                    .size(12.0)
                    .color(theme::TEXT),
                );
                if self.export_rx.is_some() && action == PendingAction::Quit {
                    ui.add_space(4.0);
                    theme::muted(ui, "A running export will be cancelled.");
                }
                if let Some(error) = &self.dialog_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let save = egui::Button::new(RichText::new("Save").strong().color(theme::BG))
                        .fill(theme::ACCENT);
                    if ui.add(save).clicked() {
                        self.dialog_error = None;
                        self.after_save = Some(action.clone());
                        self.save_current(ctx);
                    }
                    if ui.button("Don't save").clicked() {
                        self.dialog = Dialog::None;
                        self.dialog_error = None;
                        self.after_save = None;
                        self.perform(ctx, action.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        self.close_dialog();
                    }
                });
            });
        if !window_open {
            self.close_dialog();
        }
    }

    pub(super) fn dialog_recover(&mut self, ctx: &egui::Context, recovery: PathBuf) {
        egui::Window::new("Recover unsaved edits?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Splice found an autosave newer than this project, which usually means it closed before the last edits were saved.",
                    )
                    .size(12.0)
                    .color(theme::TEXT),
                );
                if let Some(error) = &self.dialog_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(10.5).color(theme::DANGER));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let restore =
                        egui::Button::new(RichText::new("Recover").strong().color(theme::BG))
                            .fill(theme::ACCENT);
                    if ui.add(restore).clicked() {
                        self.recover_from(&recovery);
                    }
                    if ui.button("Discard autosave").clicked() {
                        self.clear_autosave();
                        self.recovery_unresolved = false;
                        self.dialog = Dialog::None;
                        self.dialog_error = None;
                    }
                });
            });
    }

    pub(super) fn dialog_missing(&mut self, ctx: &egui::Context) {
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

    pub(super) fn dialog_relink(&mut self, ctx: &egui::Context, id: &str) {
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
                self.browse_row(ui, PickTarget::Relink);
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

    pub(super) fn dialog_about(&mut self, ctx: &egui::Context) {
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
                        crate::model::FPS,
                        crate::model::SAMPLE_RATE
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

    pub(super) fn start_export(&mut self) {
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
        let control = RenderControl::new();
        let worker_control = control.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = export_project_with(
                &project,
                &project_dir,
                &tools,
                preset,
                &output,
                &worker_control,
            )
            .map_err(|error| {
                if error.downcast_ref::<Cancelled>().is_some() {
                    ExportFailure::Cancelled
                } else {
                    ExportFailure::Failed(format!("{error:#}"))
                }
            });
            let _ = tx.send(result);
        });
        self.export_rx = Some(rx);
        self.export_control = Some(control);
        self.export_total = self.duration();
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

    pub(super) fn dialog_automovie(&mut self, ctx: &egui::Context) {
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

    pub(super) fn dialog_title(&mut self, ctx: &egui::Context) {
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

    pub(super) fn dialog_voiceover(&mut self, ctx: &egui::Context) {
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

    pub(super) fn start_voice_recording(&mut self) {
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

    pub(super) fn stop_voice_recording(&mut self) {
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
