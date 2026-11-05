use std::path::Path;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke,
    TextStyle, Visuals,
};

pub const BG: Color32 = Color32::from_rgb(9, 12, 17);
pub const PANEL: Color32 = Color32::from_rgb(16, 21, 29);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(22, 29, 39);
pub const CARD: Color32 = Color32::from_rgb(28, 37, 50);
pub const CARD_HOVER: Color32 = Color32::from_rgb(37, 48, 64);
pub const BORDER: Color32 = Color32::from_rgb(43, 55, 72);
pub const TEXT: Color32 = Color32::from_rgb(232, 238, 246);
pub const MUTED: Color32 = Color32::from_rgb(140, 154, 172);
pub const ACCENT: Color32 = Color32::from_rgb(255, 177, 92);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(99, 68, 35);
pub const BLUE: Color32 = Color32::from_rgb(92, 200, 255);
pub const GREEN: Color32 = Color32::from_rgb(94, 226, 181);
pub const PINK: Color32 = Color32::from_rgb(244, 91, 105);
pub const GOLD: Color32 = Color32::from_rgb(240, 201, 122);
pub const DANGER: Color32 = Color32::from_rgb(255, 107, 122);

pub const VIDEO_CLIP: Color32 = Color32::from_rgb(36, 66, 88);
pub const OVERLAY_CLIP: Color32 = Color32::from_rgb(54, 54, 88);
pub const MUSIC_CLIP: Color32 = Color32::from_rgb(34, 76, 64);
pub const VOICE_CLIP: Color32 = Color32::from_rgb(76, 45, 66);
pub const TITLE_CLIP: Color32 = Color32::from_rgb(92, 72, 38);

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut style = (*ctx.style()).clone();
    let mut visuals = Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL_RAISED;
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = PANEL_RAISED;
    visuals.code_bg_color = Color32::from_rgb(12, 16, 23);
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = BLUE;
    visuals.warn_fg_color = ACCENT;
    visuals.error_fg_color = DANGER;
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.window_shadow = Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(160),
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.slider_trailing_fill = true;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.weak_bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);

    visuals.widgets.hovered.bg_fill = CARD_HOVER;
    visuals.widgets.hovered.weak_bg_fill = CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(70, 88, 110));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);

    visuals.widgets.active.bg_fill = ACCENT_DIM;
    visuals.widgets.active.weak_bg_fill = ACCENT_DIM;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);

    visuals.widgets.open.bg_fill = CARD_HOVER;
    visuals.widgets.open.weak_bg_fill = CARD_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.open.corner_radius = CornerRadius::same(6);

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size = egui::vec2(34.0, 24.0);
    style.spacing.slider_width = 116.0;
    style.spacing.combo_width = 150.0;
    style.spacing.indent = 14.0;
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(20.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(11.5)),
    ]
    .into();
    ctx.set_style(style);
}

fn install_fonts(ctx: &egui::Context) {
    let candidates: &[(&str, &[&str])] = &[
        (
            "ui",
            &[
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            ],
        ),
        (
            "ui-bold",
            &[
                "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
                "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
            ],
        ),
        (
            "mono",
            &[
                "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
                "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
            ],
        ),
    ];

    let mut fonts = FontDefinitions::default();
    for (name, paths) in candidates {
        let Some(path) = paths.iter().map(Path::new).find(|path| path.is_file()) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        fonts
            .font_data
            .insert((*name).to_string(), FontData::from_owned(bytes).into());
        let family = if *name == "mono" {
            FontFamily::Monospace
        } else {
            FontFamily::Proportional
        };
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, (*name).to_string());
    }
    ctx.set_fonts(fonts);
}

pub fn heading(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(
        egui::RichText::new(text.into())
            .size(15.0)
            .strong()
            .color(TEXT),
    );
}

pub fn section_label(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text.into().to_uppercase())
            .size(10.5)
            .strong()
            .color(MUTED),
    );
}

pub fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).size(12.0).color(MUTED));
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    let text = egui::RichText::new(text.to_uppercase())
        .size(9.5)
        .strong()
        .color(color);
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.55)))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(text);
        });
}

pub fn kind_color(kind: crate::model::MediaKind) -> Color32 {
    match kind {
        crate::model::MediaKind::Video => BLUE,
        crate::model::MediaKind::Image => GOLD,
        crate::model::MediaKind::Audio => GREEN,
    }
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL_RAISED)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
}

pub fn format_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let total = (seconds * 1000.0).round() as u64;
    let millis = total % 1000;
    let total_seconds = total / 1000;
    let secs = total_seconds % 60;
    let minutes = (total_seconds / 60) % 60;
    let hours = total_seconds / 3600;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}.{millis:03}")
    } else {
        format!("{minutes:02}:{secs:02}.{millis:03}")
    }
}

pub fn short_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    if seconds >= 60.0 {
        format!("{:.0}:{:04.1}", (seconds / 60.0).floor(), seconds % 60.0)
    } else {
        format!("{seconds:.1}s")
    }
}
