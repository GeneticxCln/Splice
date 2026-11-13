use super::*;

pub(super) fn title_editor(ui: &mut Ui, title: &mut Title, changed: &mut bool) {
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
    if let Some(warning) = titles::font_fallback_warning(&title.font) {
        ui.label(RichText::new(warning).size(9.5).color(theme::ACCENT));
    }

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

    ui.add_space(6.0);
    ui.separator();
    theme::section_label(ui, "Motion");
    let mut custom_motion = title.motion.is_some();
    if ui
        .checkbox(&mut custom_motion, "Custom motion")
        .on_hover_text("Override the selected title style's built-in animation")
        .changed()
    {
        title.motion = custom_motion.then(TitleMotion::default);
        *changed = true;
    }
    if let Some(motion) = &mut title.motion {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Preset").size(10.0).color(theme::MUTED));
            egui::ComboBox::from_id_salt(ui.id().with("title-motion"))
                .selected_text(catalog::title_motion_name(motion.preset))
                .width(180.0)
                .show_ui(ui, |ui| {
                    for (name, preset) in TITLE_MOTIONS {
                        if ui
                            .selectable_label(motion.preset == *preset, *name)
                            .clicked()
                        {
                            motion.preset = *preset;
                            *changed = true;
                        }
                    }
                });
        });
        ui.label(
            RichText::new(catalog::title_motion_description(motion.preset))
                .size(9.5)
                .color(theme::MUTED),
        );

        let response = ui.add(
            egui::Slider::new(&mut motion.in_duration, 0.0..=3.0)
                .text("In")
                .suffix(" s"),
        );
        *changed |= response.changed();
        let response = ui.add(
            egui::Slider::new(&mut motion.out_duration, 0.0..=3.0)
                .text("Out")
                .suffix(" s"),
        );
        *changed |= response.changed();

        if motion.preset == TitleMotionPreset::Slide {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Direction").size(10.0).color(theme::MUTED));
                egui::ComboBox::from_id_salt(ui.id().with("title-motion-direction"))
                    .selected_text(motion_direction_name(motion.direction))
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (name, direction) in [
                            ("Left", MotionDirection::Left),
                            ("Right", MotionDirection::Right),
                            ("Up", MotionDirection::Up),
                            ("Down", MotionDirection::Down),
                        ] {
                            if ui
                                .selectable_label(motion.direction == direction, name)
                                .clicked()
                            {
                                motion.direction = direction;
                                *changed = true;
                            }
                        }
                    });
            });
        }

        if motion.preset != TitleMotionPreset::Typewriter {
            let response = ui.add(
                egui::Slider::new(&mut motion.distance, 0.0..=0.5)
                    .text("Distance")
                    .step_by(0.01),
            );
            *changed |= response.changed();
        }

        if matches!(
            motion.preset,
            TitleMotionPreset::Pop | TitleMotionPreset::Blur | TitleMotionPreset::Tracking
        ) {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Easing").size(10.0).color(theme::MUTED));
                egui::ComboBox::from_id_salt(ui.id().with("title-motion-easing"))
                    .selected_text(motion_easing_name(motion.easing))
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (name, easing) in [
                            ("Linear", MotionEasing::Linear),
                            ("Ease in", MotionEasing::EaseIn),
                            ("Ease out", MotionEasing::EaseOut),
                        ] {
                            if ui.selectable_label(motion.easing == easing, name).clicked() {
                                motion.easing = easing;
                                *changed = true;
                            }
                        }
                    });
            });
        }

        if motion.preset == TitleMotionPreset::Typewriter {
            theme::muted(ui, "Characters reveal across the In duration.");
        }
    }
}

pub(super) fn motion_direction_name(direction: MotionDirection) -> &'static str {
    match direction {
        MotionDirection::Left => "Left",
        MotionDirection::Right => "Right",
        MotionDirection::Up => "Up",
        MotionDirection::Down => "Down",
    }
}

pub(super) fn motion_easing_name(easing: MotionEasing) -> &'static str {
    match easing {
        MotionEasing::Linear => "Linear",
        MotionEasing::EaseIn => "Ease in",
        MotionEasing::EaseOut => "Ease out",
        MotionEasing::EaseInOut => "Ease in/out",
    }
}

pub(super) fn motion_editor(
    ui: &mut Ui,
    motion: &mut ClipMotion,
    duration: f64,
    local_playhead: f64,
    changed: &mut bool,
) {
    theme::section_label(ui, "Motion");
    if motion.keyframes.is_empty() {
        theme::muted(
            ui,
            "No keyframes. Add one at the playhead to animate scale, pan, rotation, or opacity.",
        );
    }

    let mut remove = None;
    let mut edited = false;
    let origin = motion.origin;
    for (index, keyframe) in motion.keyframes.iter_mut().enumerate() {
        let mut local_time = (keyframe.time - origin).clamp(0.0, duration);
        egui::Frame::new()
            .fill(theme::PANEL_RAISED)
            .stroke(Stroke::new(1.0_f32, theme::BORDER))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(7))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Keyframe {}", index + 1))
                            .size(11.0)
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            remove = Some(index);
                        }
                    });
                });
                if ui
                    .add(
                        egui::Slider::new(&mut local_time, 0.0..=duration)
                            .text("Time")
                            .suffix(" s"),
                    )
                    .changed()
                {
                    keyframe.time = origin + local_time;
                    edited = true;
                }
                if ui
                    .add(
                        egui::Slider::new(&mut keyframe.scale, 1.0..=2.0)
                            .text("Scale")
                            .step_by(0.01),
                    )
                    .changed()
                {
                    edited = true;
                }
                if ui
                    .add(
                        egui::Slider::new(&mut keyframe.pan_x, -1.0..=1.0)
                            .text("Pan X")
                            .step_by(0.01),
                    )
                    .changed()
                {
                    edited = true;
                }
                if ui
                    .add(
                        egui::Slider::new(&mut keyframe.pan_y, -1.0..=1.0)
                            .text("Pan Y")
                            .step_by(0.01),
                    )
                    .changed()
                {
                    edited = true;
                }
                if ui
                    .add(
                        egui::Slider::new(&mut keyframe.rotation, -180.0..=180.0)
                            .text("Rotation")
                            .suffix("°")
                            .step_by(1.0),
                    )
                    .changed()
                {
                    edited = true;
                }
                if ui
                    .add(
                        egui::Slider::new(&mut keyframe.opacity, 0.0..=1.0)
                            .text("Opacity")
                            .step_by(0.01),
                    )
                    .changed()
                {
                    edited = true;
                }
            });
        ui.add_space(4.0);
    }
    if let Some(index) = remove {
        motion.keyframes.remove(index);
        *changed = true;
    }
    if motion.keyframes.is_empty() {
        motion.origin = 0.0;
    }

    ui.horizontal(|ui| {
        if ui.button("+ Add keyframe at playhead").clicked() {
            let value = motion.sample(local_playhead.clamp(0.0, duration));
            motion.set_keyframe(local_playhead.clamp(0.0, duration), value);
            *changed = true;
        }
        if !motion.keyframes.is_empty() && ui.button("Clear").clicked() {
            motion.keyframes.clear();
            motion.origin = 0.0;
            *changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Easing").size(10.0).color(theme::MUTED));
        egui::ComboBox::from_id_salt("clip-motion-easing")
            .selected_text(motion_easing_name(motion.easing))
            .show_ui(ui, |ui| {
                for easing in [
                    MotionEasing::Linear,
                    MotionEasing::EaseIn,
                    MotionEasing::EaseOut,
                    MotionEasing::EaseInOut,
                ] {
                    if ui
                        .selectable_value(&mut motion.easing, easing, motion_easing_name(easing))
                        .changed()
                    {
                        *changed = true;
                    }
                }
            });
    });
    if edited {
        *changed = true;
        motion.sort_and_dedupe();
    }
    theme::muted(
        ui,
        "Keyframes use clip-local time. Scale 1.0 keeps the original framing; opacity 1.0 is fully visible.",
    );
}

pub(super) fn color_row(ui: &mut Ui, label: &str, value: &mut String, changed: &mut bool) {
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

pub(super) fn audio_editor(
    ui: &mut Ui,
    audio: &mut AudioParams,
    duration: f64,
    changed: &mut bool,
) {
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

pub(super) fn fit_rect(bounds: Rect, aspect: f32) -> Rect {
    let mut width = bounds.width();
    let mut height = width / aspect;
    if height > bounds.height() {
        height = bounds.height();
        width = height * aspect;
    }
    Rect::from_center_size(bounds.center(), Vec2::new(width.max(1.0), height.max(1.0)))
}

pub(super) fn paint_short_safe_areas(painter: &egui::Painter, frame: Rect) {
    let left = frame.left() + frame.width() * 0.15;
    let top = frame.top() + frame.height() * 0.10;
    let bottom = frame.bottom() - frame.height() * 0.20;
    let right = frame.right() - frame.width() * 0.15;
    let safe = Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom));
    let shade = Color32::from_black_alpha(35);
    painter.rect_filled(
        Rect::from_min_max(frame.min, egui::pos2(frame.right(), top)),
        egui::CornerRadius::ZERO,
        shade,
    );
    painter.rect_filled(
        Rect::from_min_max(egui::pos2(frame.left(), bottom), frame.max),
        egui::CornerRadius::ZERO,
        shade,
    );
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(frame.left(), frame.top()),
            egui::pos2(left, frame.bottom()),
        ),
        egui::CornerRadius::ZERO,
        shade,
    );
    painter.rect_filled(
        Rect::from_min_max(
            egui::pos2(right, frame.top()),
            egui::pos2(frame.right(), frame.bottom()),
        ),
        egui::CornerRadius::ZERO,
        shade,
    );
    painter.rect_stroke(
        safe,
        egui::CornerRadius::ZERO,
        Stroke::new(1.0_f32, theme::GOLD),
        egui::StrokeKind::Inside,
    );
}
