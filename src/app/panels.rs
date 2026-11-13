use super::editors::*;
use super::*;

impl SpliceApp {
    pub(super) fn top_menu(&mut self, ctx: &egui::Context) {
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
                            self.request(ctx, PendingAction::NewProject);
                            ui.close_menu();
                        }
                        if ui.button("New Short project").clicked() {
                            self.request(ctx, PendingAction::NewShort);
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
                            self.request(ctx, PendingAction::Quit);
                            ui.close_menu();
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
                        ui.separator();
                        ui.checkbox(&mut self.shorts_safe_area, "Shorts safe areas");
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("About Splice").clicked() {
                            self.open_dialog(Dialog::About);
                            ui.close_menu();
                        }
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let warnings = self
                            .tools
                            .as_ref()
                            .map(Toolchain::capability_warnings)
                            .unwrap_or_default();
                        let (label, color) = if self.tools.is_none() {
                            ("FFMPEG MISSING", theme::DANGER)
                        } else if warnings.is_empty() {
                            ("FFMPEG READY", theme::GREEN)
                        } else {
                            ("FFMPEG LIMITED", theme::GOLD)
                        };
                        let response = ui.label(
                            RichText::new(format!("● {label}"))
                                .size(9.5)
                                .strong()
                                .color(color),
                        );
                        if let Some(tools) = &self.tools {
                            let summary = if warnings.is_empty() {
                                tools.version_summary()
                            } else {
                                format!(
                                    "{}\nMissing: {}",
                                    tools.version_summary(),
                                    warnings.join("; ")
                                )
                            };
                            response.on_hover_text(summary);
                        } else if let Some(error) = &self.tool_error {
                            response.on_hover_text(error);
                        }
                    });
                });
            });
    }

    pub(super) fn toolbar(&mut self, ctx: &egui::Context) {
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
                        self.request(ctx, PendingAction::NewProject);
                    }
                    if ui.button("New Short").clicked() {
                        self.request(ctx, PendingAction::NewShort);
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

    pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
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

    pub(super) fn timeline_panel(&mut self, ctx: &egui::Context) {
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

    pub(super) fn media_panel(&mut self, ctx: &egui::Context) {
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

    pub(super) fn preview_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ctx, |ui| {
                let duration = self.duration();
                let canvas_width = self.project.settings.width;
                let canvas_height = self.project.settings.height;
                let canvas_aspect = self.project.settings.aspect_ratio();
                let is_short = self.project.settings.is_short();
                let (proxy_width, proxy_height) = preview_dimensions(canvas_width, canvas_height);
                let window = self.preview.as_ref().and_then(|preview| preview.window());
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
                    theme::badge(
                        ui,
                        &format!("Proxy {proxy_width}×{proxy_height}"),
                        theme::BLUE,
                    );
                    if let Some((start, end)) = window {
                        theme::badge(
                            ui,
                            &format!(
                                "Slice {} – {}",
                                theme::short_time(start),
                                theme::short_time(end)
                            ),
                            theme::GOLD,
                        );
                    }
                    if is_short {
                        let (label, color) = if duration > 180.0 {
                            ("Shorts over 3:00", theme::GOLD)
                        } else {
                            ("Shorts 3:00 target", theme::GREEN)
                        };
                        theme::badge(ui, label, color);
                    }
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
                let target = fit_rect(outer.shrink(8.0), canvas_aspect);
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
                if self.shorts_safe_area && is_short {
                    paint_short_safe_areas(&painter, target);
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
                    format!(
                        "{canvas_width} × {canvas_height} · {} fps",
                        crate::model::FPS
                    ),
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

    pub(super) fn inspector_panel(&mut self, ctx: &egui::Context) {
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

    pub(super) fn inspector_contents(&mut self, ui: &mut Ui) {
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
        let destinations = edit::valid_destinations(&self.project, &selection);
        let mut move_to: Option<String> = None;
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

        let local_playhead = self.playhead();
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
                        ui.add(
                            egui::TextEdit::singleline(&mut self.transition_filter)
                                .hint_text("Search transitions…")
                                .desired_width(220.0),
                        );
                        let query = self.transition_filter.trim().to_lowercase();
                        let mut shown = 0usize;
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                if query.is_empty()
                                    && ui
                                        .selectable_label(clip.transition.is_none(), "None")
                                        .clicked()
                                {
                                    clip.transition = None;
                                    changed = true;
                                }
                                for (name, kind) in TRANSITIONS {
                                    if !query.is_empty() && !name.to_lowercase().contains(&query) {
                                        continue;
                                    }
                                    shown += 1;
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
                                        self.transition_filter.clear();
                                        changed = true;
                                    }
                                }
                            });
                        if shown == 0 && !query.is_empty() {
                            theme::muted(ui, "No transitions match that search.");
                        }
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
                ui.add(
                    egui::TextEdit::singleline(&mut self.effect_filter)
                        .hint_text("Search effects…")
                        .desired_width(240.0),
                );
                let query = self.effect_filter.trim().to_lowercase();
                let mut shown = 0usize;
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        for (category, kinds) in catalog::EFFECT_CATEGORIES {
                            let mut category_header = false;
                            for def in EFFECTS {
                                if !kinds.contains(&def.kind) {
                                    continue;
                                }
                                if !query.is_empty()
                                    && !def.name.to_lowercase().contains(&query)
                                    && !def.description.to_lowercase().contains(&query)
                                {
                                    continue;
                                }
                                if !category_header {
                                    theme::section_label(ui, *category);
                                    category_header = true;
                                }
                                shown += 1;
                                let response = ui.button(def.name).on_hover_text(def.description);
                                if response.clicked() {
                                    if let Some(effect) = catalog::make_effect(def.kind) {
                                        clip.effects.push(effect);
                                        changed = true;
                                    }
                                    self.effect_filter.clear();
                                    ui.close_menu();
                                }
                            }
                        }
                    });
                if shown == 0 {
                    theme::muted(ui, "No effects match that search.");
                }
            });
        }

        if track_kind != TrackKind::Audio && clip.title.is_none() {
            ui.add_space(6.0);
            ui.separator();
            motion_editor(ui, &mut clip.motion, duration, local_playhead, &mut changed);
        }

        if track_kind == TrackKind::Overlay && clip.title.is_none() {
            ui.add_space(6.0);
            ui.separator();
            placement_editor(ui, &mut clip.placement, &mut changed);
        }

        if !destinations.is_empty() {
            ui.add_space(6.0);
            ui.separator();
            theme::section_label(ui, "Track");
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Move to").size(10.5).color(theme::MUTED));
                for (id, name) in &destinations {
                    if ui.button(name).clicked() {
                        move_to = Some(id.clone());
                    }
                }
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
        if let Some(track_id) = move_to {
            let before = self.project.clone();
            match edit::move_clip_to_track(&mut self.project, &selection, &track_id) {
                Ok(()) => {
                    self.push_undo_snapshot(before);
                    self.mark_edited();
                    self.set_status("Clip moved to another track", false);
                }
                Err(error) => self.set_status(format!("{error:#}"), true),
            }
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

    pub(super) fn project_inspector(&mut self, ui: &mut Ui) {
        let snapshot = self.project.clone();
        let mut changed = false;
        let is_short = self.project.settings.is_short();
        let mut switch_short = None;
        let mut switch_framing = None;
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
                        crate::model::FPS
                    ))
                    .size(10.5),
                );
                ui.end_row();
                ui.label(RichText::new("Layout").size(10.0).color(theme::MUTED));
                ui.horizontal(|ui| {
                    if ui.selectable_label(!is_short, "Landscape").clicked() {
                        switch_short = Some(false);
                    }
                    if ui.selectable_label(is_short, "Shorts").clicked() {
                        switch_short = Some(true);
                    }
                });
                ui.end_row();
                ui.label(RichText::new("Framing").size(10.0).color(theme::MUTED));
                let framing = self.project.settings.framing;
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(framing == Framing::Fit, "Fit")
                        .clicked()
                    {
                        switch_framing = Some(Framing::Fit);
                    }
                    if ui
                        .selectable_label(framing == Framing::Fill, "Fill")
                        .clicked()
                    {
                        switch_framing = Some(Framing::Fill);
                    }
                });
                ui.end_row();
                ui.label(RichText::new("Audio").size(10.0).color(theme::MUTED));
                ui.label(
                    RichText::new(format!("{} Hz · stereo", crate::model::SAMPLE_RATE)).size(10.5),
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

        if let Some(short) = switch_short {
            self.project.settings = if short {
                Settings::shorts()
            } else {
                Settings::default()
            };
            self.export_preset = ExportPreset::matching(&self.project.settings);
            self.title_draft = Title::opening_for(&self.project.name, &self.project.settings);
            self.invalidate_preview();
            changed = true;
        }
        if let Some(framing) = switch_framing {
            if self.project.settings.framing != framing {
                self.project.settings.framing = framing;
                self.invalidate_preview();
                changed = true;
            }
        }

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
}
