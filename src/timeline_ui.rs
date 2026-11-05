use std::collections::HashSet;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind,
    Ui, Vec2,
};

use crate::catalog::{TRANSITIONS, transition_name};
use crate::model::{Clip, MediaKind, Project, Track, TrackKind, TransitionKind};
use crate::theme;

pub const RULER_HEIGHT: f32 = 26.0;
pub const TRACK_HEIGHT: f32 = 54.0;
pub const LABEL_WIDTH: f32 = 118.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TimelineView {
    Storyboard,
    #[default]
    Timeline,
}

impl TimelineView {
    pub fn label(self) -> &'static str {
        match self {
            TimelineView::Storyboard => "Storyboard",
            TimelineView::Timeline => "Timeline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimEdge {
    Start,
    End,
}

#[derive(Debug, Clone)]
pub struct ClipDragState {
    pub clip_id: String,
    pub grab_offset: f64,
}

#[derive(Debug, Clone)]
pub struct TrimDragState {
    pub clip_id: String,
    pub edge: TrimEdge,
    pub original_start: f64,
    pub original_in: f64,
    pub original_out: f64,
    pub source_max: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct StoryboardDragState {
    pub clip_id: String,
    pub target_index: usize,
    pub insert_x: f32,
}

#[derive(Debug, Default)]
pub struct TimelineUiState {
    pub view: TimelineView,
    pub zoom: f32,
    pub snap: bool,
    pub drag: Option<ClipDragState>,
    pub trim: Option<TrimDragState>,
    pub storyboard: Option<StoryboardDragState>,
}

impl TimelineUiState {
    pub fn new() -> Self {
        Self {
            view: TimelineView::Timeline,
            zoom: 46.0,
            snap: true,
            drag: None,
            trim: None,
            storyboard: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TimelineCommand {
    SplitAtPlayhead(String),
    Duplicate(String),
    Delete(String),
    RemoveTransition(String),
    SetTransition(String, TransitionKind),
    MoveLeft(String),
    MoveRight(String),
}

#[derive(Debug, Clone)]
pub enum TimelineAction {
    SelectClip(String),
    ClearSelection,
    Seek(f64),
    MoveClip {
        clip_id: String,
        start: f64,
    },
    FinishClipMove(String),
    TrimClip {
        clip_id: String,
        start: f64,
        in_point: f64,
        out_point: f64,
    },
    FinishTrim(String),
    ReorderPrimary {
        clip_id: String,
        target_index: usize,
    },
    Command(TimelineCommand),
}

pub fn show(
    ui: &mut Ui,
    state: &mut TimelineUiState,
    project: &Project,
    project_dir: &std::path::Path,
    playhead: f64,
    selected: Option<&str>,
    missing: &HashSet<String>,
) -> Vec<TimelineAction> {
    match state.view {
        TimelineView::Storyboard => {
            show_storyboard(ui, state, project, project_dir, selected, missing)
        }
        TimelineView::Timeline => {
            show_tracks(ui, state, project, project_dir, playhead, selected, missing)
        }
    }
}

fn is_missing(missing: &HashSet<String>, clip: &Clip) -> bool {
    clip.media_id
        .as_deref()
        .map(|id| missing.contains(id))
        .unwrap_or(false)
}

fn clip_color(track: &Track, clip: &Clip) -> Color32 {
    if clip.title.is_some() {
        theme::TITLE_CLIP
    } else {
        match track.kind {
            TrackKind::Video => theme::VIDEO_CLIP,
            TrackKind::Overlay => theme::OVERLAY_CLIP,
            TrackKind::Audio => {
                if track.id.contains("voice") {
                    theme::VOICE_CLIP
                } else {
                    theme::MUSIC_CLIP
                }
            }
        }
    }
}

fn show_storyboard(
    ui: &mut Ui,
    state: &mut TimelineUiState,
    project: &Project,
    project_dir: &std::path::Path,
    selected: Option<&str>,
    missing: &HashSet<String>,
) -> Vec<TimelineAction> {
    let mut actions = Vec::new();
    let _ = project_dir;
    let clips: Vec<Clip> = project
        .primary_video_track()
        .map(|track| track.clips.clone())
        .unwrap_or_default();

    if clips.is_empty() {
        theme::muted(
            ui,
            "Drop video, photos or audio here, then press “Add to timeline” in the library.",
        );
        ui.add_space(8.0);
        return actions;
    }

    let card_size = Vec2::new(148.0, 102.0);
    let connector_width = 34.0;
    let mut rects: Vec<(String, Rect)> = Vec::new();

    ScrollArea::horizontal()
        .id_salt("storyboard-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (index, clip) in clips.iter().enumerate() {
                    if index > 0 {
                        let transition = clip.transition.as_ref();
                        let (connector_rect, connector_resp) = ui.allocate_exact_size(
                            Vec2::new(connector_width, card_size.y),
                            Sense::hover(),
                        );
                        let painter = ui.painter();
                        let center = connector_rect.center();
                        painter.line_segment(
                            [
                                Pos2::new(connector_rect.left(), center.y),
                                Pos2::new(connector_rect.right(), center.y),
                            ],
                            Stroke::new(1.0_f32, theme::BORDER),
                        );
                        if let Some(transition) = transition {
                            let color = theme::ACCENT;
                            painter.circle_filled(center, 9.0, theme::PANEL_RAISED);
                            painter.circle_stroke(center, 9.0, Stroke::new(1.2_f32, color));
                            painter.circle_filled(center, 3.5, color);
                            connector_resp.on_hover_text(format!(
                                "{} · {:.2}s",
                                transition_name(transition.kind),
                                transition.duration
                            ));
                        } else {
                            painter.circle_filled(center, 3.0, theme::BORDER);
                        }
                    }

                    let (rect, response) =
                        ui.allocate_exact_size(card_size, Sense::click_and_drag());
                    let selected = selected == Some(clip.id.as_str());
                    let drag_active = state
                        .storyboard
                        .as_ref()
                        .map(|drag| drag.clip_id == clip.id)
                        .unwrap_or(false);
                    paint_storyboard_card(
                        ui,
                        rect,
                        clip,
                        index,
                        is_missing(missing, clip),
                        drag_active,
                        selected,
                    );
                    rects.push((clip.id.clone(), rect));

                    if response.clicked() {
                        actions.push(TimelineAction::SelectClip(clip.id.clone()));
                    }
                    if response.double_clicked() {
                        actions.push(TimelineAction::Seek(clip.start));
                    }
                    if response.drag_started() {
                        state.storyboard = Some(StoryboardDragState {
                            clip_id: clip.id.clone(),
                            target_index: index,
                            insert_x: rect.center().x,
                        });
                    }
                    if response.dragged() {
                        if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                            let mut target = 0usize;
                            let mut insert_x = rects
                                .last()
                                .map(|(_, rect)| rect.right() + connector_width * 0.5)
                                .unwrap_or(rect.left());
                            for (other_id, other_rect) in &rects {
                                if *other_id == clip.id {
                                    continue;
                                }
                                if pointer.x > other_rect.center().x {
                                    target += 1;
                                    insert_x = other_rect.right() + connector_width * 0.5;
                                } else if target == 0 {
                                    insert_x = other_rect.left() - connector_width * 0.5;
                                }
                            }
                            if target == rects.len().saturating_sub(1)
                                && pointer.x > rect.center().x
                            {
                                insert_x = rect.right() + connector_width * 0.5;
                            }
                            state.storyboard = Some(StoryboardDragState {
                                clip_id: clip.id.clone(),
                                target_index: target,
                                insert_x,
                            });
                        }
                    }
                    if response.drag_stopped() {
                        if let Some(drag) = state.storyboard.take() {
                            actions.push(TimelineAction::ReorderPrimary {
                                clip_id: drag.clip_id,
                                target_index: drag.target_index,
                            });
                        }
                    }

                    let clip_id = clip.id.clone();
                    response.context_menu(|ui| {
                        if ui.button("Select").clicked() {
                            actions.push(TimelineAction::SelectClip(clip_id.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Seek here").clicked() {
                            actions.push(TimelineAction::Seek(clip.start));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Split at playhead").clicked() {
                            actions.push(TimelineAction::Command(
                                TimelineCommand::SplitAtPlayhead(clip_id.clone()),
                            ));
                            ui.close_menu();
                        }
                        if ui.button("Duplicate").clicked() {
                            actions.push(TimelineAction::Command(TimelineCommand::Duplicate(
                                clip_id.clone(),
                            )));
                            ui.close_menu();
                        }
                        if ui.button("Delete").clicked() {
                            actions.push(TimelineAction::Command(TimelineCommand::Delete(
                                clip_id.clone(),
                            )));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Move left").clicked() {
                            actions.push(TimelineAction::Command(TimelineCommand::MoveLeft(
                                clip_id.clone(),
                            )));
                            ui.close_menu();
                        }
                        if ui.button("Move right").clicked() {
                            actions.push(TimelineAction::Command(TimelineCommand::MoveRight(
                                clip_id.clone(),
                            )));
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.menu_button("Incoming transition", |ui| {
                            if ui.button("None").clicked() {
                                actions.push(TimelineAction::Command(
                                    TimelineCommand::RemoveTransition(clip_id.clone()),
                                ));
                                ui.close_menu();
                            }
                            for (name, kind) in TRANSITIONS {
                                if ui.button(*name).clicked() {
                                    actions.push(TimelineAction::Command(
                                        TimelineCommand::SetTransition(clip_id.clone(), *kind),
                                    ));
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                }
            });

            if let Some(drag) = &state.storyboard {
                let painter = ui.painter();
                let x = drag.insert_x;
                painter.line_segment(
                    [
                        Pos2::new(x, ui.min_rect().top() + 4.0),
                        Pos2::new(x, ui.min_rect().top() + card_size.y - 4.0),
                    ],
                    Stroke::new(3.0_f32, theme::ACCENT),
                );
            }
        });

    actions
}

fn paint_storyboard_card(
    ui: &mut Ui,
    rect: Rect,
    clip: &Clip,
    index: usize,
    missing: bool,
    drag_active: bool,
    selected: bool,
) {
    let painter = ui.painter();
    let fill = if missing {
        Color32::from_rgb(54, 28, 36)
    } else {
        theme::CARD
    };
    painter.rect_filled(rect, CornerRadius::same(8), fill);
    let stroke = if drag_active || selected {
        Stroke::new(2.0_f32, theme::ACCENT)
    } else {
        Stroke::new(1.0_f32, theme::BORDER)
    };
    painter.rect_stroke(rect, CornerRadius::same(8), stroke, StrokeKind::Inside);

    let thumb = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 58.0));
    let tint = clip_tint(clip);
    painter.rect_filled(thumb, CornerRadius::same(8), tint.gamma_multiply(0.55));
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(thumb.left(), thumb.bottom() - 24.0),
            Vec2::new(thumb.width(), 24.0),
        ),
        CornerRadius::ZERO,
        Color32::from_black_alpha(50),
    );
    painter.text(
        Pos2::new(thumb.left() + 8.0, thumb.top() + 7.0),
        Align2::LEFT_TOP,
        format!("{:02}", index + 1),
        FontId::monospace(10.0),
        theme::TEXT,
    );
    let kind = if clip.title.is_some() {
        "TITLE"
    } else if missing {
        "OFFLINE"
    } else if clip.is_title() {
        "TITLE"
    } else {
        "CLIP"
    };
    painter.text(
        Pos2::new(thumb.right() - 8.0, thumb.top() + 7.0),
        Align2::RIGHT_TOP,
        kind,
        FontId::proportional(9.5),
        if missing { theme::DANGER } else { theme::TEXT },
    );
    let icon = if clip.title.is_some() { "T" } else { "▶" };
    painter.text(
        thumb.center() + Vec2::new(0.0, 2.0),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(20.0),
        Color32::from_white_alpha(190),
    );

    let name = if clip.name.trim().is_empty() {
        "Untitled"
    } else {
        clip.name.as_str()
    };
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.top() + 64.0),
        Align2::LEFT_TOP,
        truncate(name, 20),
        FontId::proportional(12.0),
        theme::TEXT,
    );
    let mut meta = format!("{:.1}s", clip.duration());
    if !clip.effects.is_empty() {
        meta.push_str(&format!("  ·  {} fx", clip.effects.len()));
    }
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.top() + 82.0),
        Align2::LEFT_TOP,
        meta,
        FontId::proportional(10.0),
        theme::MUTED,
    );
    if let Some(transition) = &clip.transition {
        painter.text(
            Pos2::new(rect.right() - 8.0, rect.top() + 82.0),
            Align2::RIGHT_TOP,
            format!("◆ {:.1}", transition.duration),
            FontId::proportional(10.0),
            theme::ACCENT,
        );
    }
    if missing {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "MEDIA OFFLINE",
            FontId::proportional(12.0),
            theme::DANGER,
        );
    }
}

fn clip_tint(clip: &Clip) -> Color32 {
    let mut hash = 2166136261u32;
    for byte in clip.name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    let r = 70u8.saturating_add((hash & 0x3f) as u8);
    let g = 70u8.saturating_add(((hash >> 8) & 0x3f) as u8);
    let b = 80u8.saturating_add(((hash >> 16) & 0x3f) as u8);
    Color32::from_rgb(r, g, b)
}

fn show_tracks(
    ui: &mut Ui,
    state: &mut TimelineUiState,
    project: &Project,
    project_dir: &std::path::Path,
    playhead: f64,
    selected: Option<&str>,
    missing: &HashSet<String>,
) -> Vec<TimelineAction> {
    let mut actions = Vec::new();
    let _ = project_dir;
    let rows = project.tracks.len().max(1);
    let total_height = RULER_HEIGHT + rows as f32 * TRACK_HEIGHT;
    let duration = project.timeline_duration().max(8.0);
    let zoom = state.zoom.clamp(10.0, 320.0);
    let content_width = (duration * zoom as f64 + 240.0).max(320.0) as f32;

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (label_rect, _) =
            ui.allocate_exact_size(Vec2::new(LABEL_WIDTH, total_height), Sense::hover());
        paint_track_labels(ui, label_rect, project, rows);

        let scrolled_width = ui.available_width().max(120.0);
        ScrollArea::horizontal()
            .id_salt("timeline-scroll")
            .auto_shrink([false, false])
            .max_width(scrolled_width)
            .show(ui, |ui| {
                let width = content_width.max(ui.available_width());
                let (canvas, canvas_response) =
                    ui.allocate_exact_size(Vec2::new(width, total_height), Sense::click());
                let content_rect = Rect::from_min_size(
                    Pos2::new(canvas.left(), canvas.top()),
                    Vec2::new(content_width, total_height),
                );
                paint_tracks_background(ui, content_rect, project, rows, zoom);
                let ruler_rect = Rect::from_min_size(canvas.min, Vec2::new(width, RULER_HEIGHT));
                paint_ruler(ui, ruler_rect, zoom, duration, playhead);

                let ruler_response = ui.interact(
                    ruler_rect,
                    ui.id().with("timeline-ruler"),
                    Sense::click_and_drag(),
                );
                if ruler_response.clicked() || ruler_response.dragged() {
                    if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                        let time = (((pointer.x - canvas.left()) / zoom)
                            .clamp(0.0, duration as f32)) as f64;
                        actions.push(TimelineAction::Seek(round_time(time)));
                    }
                }

                if canvas_response.clicked() && !ruler_response.hovered() {
                    if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                        let time = (((pointer.x - canvas.left()) / zoom)
                            .clamp(0.0, duration as f32)) as f64;
                        actions.push(TimelineAction::Seek(round_time(time)));
                        actions.push(TimelineAction::ClearSelection);
                    }
                }

                let mut clip_rects: Vec<(String, Rect)> = Vec::new();
                let tracks: Vec<Track> = project.tracks.clone();
                for (row_index, track) in tracks.iter().enumerate() {
                    let row_top = canvas.top() + RULER_HEIGHT + row_index as f32 * TRACK_HEIGHT;
                    for clip in &track.clips {
                        let x = canvas.left() + clip.start as f32 * zoom;
                        let w = (clip.duration() as f32 * zoom).max(14.0);
                        let rect = Rect::from_min_size(
                            Pos2::new(x, row_top + 4.0),
                            Vec2::new(w, TRACK_HEIGHT - 8.0),
                        );
                        clip_rects.push((clip.id.clone(), rect));
                        let clip_missing = is_missing(missing, clip);
                        paint_timeline_clip(
                            ui,
                            rect,
                            clip,
                            track,
                            clip_missing,
                            state
                                .drag
                                .as_ref()
                                .map(|drag| drag.clip_id == clip.id)
                                .unwrap_or(false)
                                || state
                                    .trim
                                    .as_ref()
                                    .map(|trim| trim.clip_id == clip.id)
                                    .unwrap_or(false),
                            selected == Some(clip.id.as_str()),
                        );
                        paint_transition_marker(ui, rect, clip, zoom);
                        paint_audio_fades(ui, rect, clip);

                        let clip_resp = ui.interact(
                            rect,
                            ui.id().with(("timeline-clip", clip.id.as_str())),
                            Sense::click_and_drag(),
                        );
                        if clip_resp.clicked() {
                            actions.push(TimelineAction::SelectClip(clip.id.clone()));
                        }
                        if clip_resp.double_clicked() {
                            actions.push(TimelineAction::Seek(clip.start));
                        }
                        if clip_resp.drag_started() {
                            if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                                let time = ((pointer.x - canvas.left()) / zoom) as f64;
                                state.drag = Some(ClipDragState {
                                    clip_id: clip.id.clone(),
                                    grab_offset: (time - clip.start).max(0.0),
                                });
                            }
                        }
                        if clip_resp.dragged() {
                            if let Some(drag) = &state.drag {
                                if drag.clip_id == clip.id {
                                    if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                                        let raw = (((pointer.x - canvas.left()) / zoom) as f64)
                                            - drag.grab_offset;
                                        let start = snap_time(raw.max(0.0), state.snap);
                                        actions.push(TimelineAction::MoveClip {
                                            clip_id: clip.id.clone(),
                                            start,
                                        });
                                    }
                                }
                            }
                        }
                        if clip_resp.drag_stopped()
                            && state
                                .drag
                                .as_ref()
                                .map(|drag| drag.clip_id == clip.id)
                                .unwrap_or(false)
                        {
                            state.drag = None;
                            actions.push(TimelineAction::FinishClipMove(clip.id.clone()));
                        }

                        let clip_id = clip.id.clone();
                        let start = clip.start;
                        clip_resp.context_menu(|ui| {
                            if ui.button("Select").clicked() {
                                actions.push(TimelineAction::SelectClip(clip_id.clone()));
                                ui.close_menu();
                            }
                            if ui.button("Seek to clip").clicked() {
                                actions.push(TimelineAction::Seek(start));
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Split at playhead").clicked() {
                                actions.push(TimelineAction::Command(
                                    TimelineCommand::SplitAtPlayhead(clip_id.clone()),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Duplicate").clicked() {
                                actions.push(TimelineAction::Command(TimelineCommand::Duplicate(
                                    clip_id.clone(),
                                )));
                                ui.close_menu();
                            }
                            if ui.button("Delete").clicked() {
                                actions.push(TimelineAction::Command(TimelineCommand::Delete(
                                    clip_id.clone(),
                                )));
                                ui.close_menu();
                            }
                            ui.separator();
                            ui.menu_button("Incoming transition", |ui| {
                                if ui.button("None").clicked() {
                                    actions.push(TimelineAction::Command(
                                        TimelineCommand::RemoveTransition(clip_id.clone()),
                                    ));
                                    ui.close_menu();
                                }
                                for (name, kind) in TRANSITIONS {
                                    if ui.button(*name).clicked() {
                                        actions.push(TimelineAction::Command(
                                            TimelineCommand::SetTransition(clip_id.clone(), *kind),
                                        ));
                                        ui.close_menu();
                                    }
                                }
                            });
                        });

                        // Trim handles sit on top of the clip body.
                        let handle_w = 7.0_f32.min(w * 0.25);
                        let start_handle = Rect::from_min_size(
                            rect.min,
                            Vec2::new(handle_w.max(3.0), rect.height()),
                        );
                        let end_handle = Rect::from_min_size(
                            Pos2::new(rect.right() - handle_w.max(3.0), rect.top()),
                            Vec2::new(handle_w.max(3.0), rect.height()),
                        );
                        paint_trim_handle(ui, start_handle, true);
                        paint_trim_handle(ui, end_handle, false);

                        let start_resp = ui.interact(
                            start_handle,
                            ui.id().with(("trim-start", clip.id.as_str())),
                            Sense::drag(),
                        );
                        let end_resp = ui.interact(
                            end_handle,
                            ui.id().with(("trim-end", clip.id.as_str())),
                            Sense::drag(),
                        );
                        if start_resp.hovered() || end_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        }
                        if start_resp.drag_started() {
                            state.trim = Some(trim_state(project, clip, TrimEdge::Start));
                        }
                        if end_resp.drag_started() {
                            state.trim = Some(trim_state(project, clip, TrimEdge::End));
                        }
                        if start_resp.dragged() || end_resp.dragged() {
                            if let Some(trim) = &state.trim {
                                if trim.clip_id == clip.id {
                                    if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                                        let time = snap_time(
                                            (((pointer.x - canvas.left()) / zoom) as f64).max(0.0),
                                            state.snap,
                                        );
                                        let (new_start, new_in, new_out) = compute_trim(trim, time);
                                        actions.push(TimelineAction::TrimClip {
                                            clip_id: clip.id.clone(),
                                            start: new_start,
                                            in_point: new_in,
                                            out_point: new_out,
                                        });
                                    }
                                }
                            }
                        }
                        if (start_resp.drag_stopped() || end_resp.drag_stopped())
                            && state
                                .trim
                                .as_ref()
                                .map(|trim| trim.clip_id == clip.id)
                                .unwrap_or(false)
                        {
                            state.trim = None;
                            actions.push(TimelineAction::FinishTrim(clip.id.clone()));
                        }
                    }
                }

                // Playhead on top.
                let x = canvas.left() + playhead as f32 * zoom;
                if x >= canvas.left() - 2.0 && x <= canvas.right() + 2.0 {
                    let painter = ui.painter();
                    painter.line_segment(
                        [
                            Pos2::new(x, canvas.top() + 2.0),
                            Pos2::new(x, canvas.top() + total_height),
                        ],
                        Stroke::new(1.5_f32, theme::ACCENT),
                    );
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            Pos2::new(x - 6.0, canvas.top() + 1.0),
                            Pos2::new(x + 6.0, canvas.top() + 1.0),
                            Pos2::new(x + 6.0, canvas.top() + 9.0),
                            Pos2::new(x, canvas.top() + 15.0),
                            Pos2::new(x - 6.0, canvas.top() + 9.0),
                        ],
                        theme::ACCENT,
                        Stroke::NONE,
                    ));
                }
                let _ = &clip_rects;
            });
    });

    actions
}

fn trim_state(project: &Project, clip: &Clip, edge: TrimEdge) -> TrimDragState {
    let source_max = clip
        .media_id
        .as_deref()
        .and_then(|id| project.media_by_id(id))
        .and_then(|asset| {
            if asset.kind == MediaKind::Image {
                None
            } else if asset.duration > 0.05 {
                Some(asset.duration)
            } else {
                None
            }
        });
    TrimDragState {
        clip_id: clip.id.clone(),
        edge,
        original_start: clip.start,
        original_in: clip.in_point,
        original_out: clip.out_point,
        source_max,
    }
}

fn compute_trim(trim: &TrimDragState, time: f64) -> (f64, f64, f64) {
    match trim.edge {
        TrimEdge::End => {
            let mut out = trim.original_in + (time - trim.original_start).max(0.1);
            if let Some(max) = trim.source_max {
                out = out.min(max);
            }
            let out = out.max(trim.original_in + 0.1);
            (trim.original_start, trim.original_in, out)
        }
        TrimEdge::Start => {
            let desired_in = (trim.original_in + (time - trim.original_start))
                .clamp(0.0, trim.original_out - 0.1);
            let start = (trim.original_start + (desired_in - trim.original_in)).max(0.0);
            let in_point = trim.original_in + (start - trim.original_start);
            (start, in_point.max(0.0), trim.original_out)
        }
    }
}

fn paint_track_labels(ui: &mut Ui, rect: Rect, project: &Project, rows: usize) {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::ZERO, theme::PANEL);
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0_f32, theme::BORDER),
    );
    painter.text(
        Pos2::new(rect.left() + 12.0, rect.top() + 8.0),
        Align2::LEFT_TOP,
        "TRACKS",
        FontId::proportional(9.5),
        theme::MUTED,
    );
    for (index, track) in project.tracks.iter().enumerate().take(rows) {
        let top = rect.top() + RULER_HEIGHT + index as f32 * TRACK_HEIGHT;
        let row = Rect::from_min_size(
            Pos2::new(rect.left(), top),
            Vec2::new(rect.width(), TRACK_HEIGHT),
        );
        let color = match track.kind {
            TrackKind::Video => theme::BLUE,
            TrackKind::Overlay => theme::GOLD,
            TrackKind::Audio => {
                if track.id.contains("voice") {
                    theme::PINK
                } else {
                    theme::GREEN
                }
            }
        };
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(row.left() + 8.0, top + 12.0),
                Vec2::new(3.0, 30.0),
            ),
            CornerRadius::same(2),
            color,
        );
        painter.text(
            Pos2::new(row.left() + 18.0, top + 12.0),
            Align2::LEFT_TOP,
            track.name.clone(),
            FontId::proportional(12.5),
            theme::TEXT,
        );
        let meta = if track.muted {
            "muted".to_string()
        } else {
            format!(
                "{} clip{}",
                track.clips.len(),
                if track.clips.len() == 1 { "" } else { "s" }
            )
        };
        painter.text(
            Pos2::new(row.left() + 18.0, top + 30.0),
            Align2::LEFT_TOP,
            meta,
            FontId::proportional(10.0),
            theme::MUTED,
        );
        painter.line_segment(
            [
                Pos2::new(row.left() + 10.0, row.bottom()),
                Pos2::new(row.right() - 10.0, row.bottom()),
            ],
            Stroke::new(1.0_f32, theme::BORDER.gamma_multiply(0.55)),
        );
    }
}

fn paint_tracks_background(ui: &mut Ui, rect: Rect, project: &Project, rows: usize, zoom: f32) {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::ZERO, theme::BG);
    for index in 0..rows {
        let top = rect.top() + RULER_HEIGHT + index as f32 * TRACK_HEIGHT;
        let row = Rect::from_min_size(
            Pos2::new(rect.left(), top),
            Vec2::new(rect.width(), TRACK_HEIGHT),
        );
        let fill = if index % 2 == 0 {
            Color32::from_rgb(14, 19, 26)
        } else {
            Color32::from_rgb(12, 16, 22)
        };
        painter.rect_filled(row, CornerRadius::ZERO, fill);
        painter.line_segment(
            [row.left_bottom(), row.right_bottom()],
            Stroke::new(1.0_f32, theme::BORDER.gamma_multiply(0.45)),
        );
    }
    painter.rect_filled(
        Rect::from_min_size(rect.min, Vec2::new(rect.width(), RULER_HEIGHT)),
        CornerRadius::ZERO,
        theme::PANEL,
    );
    // Vertical second grid.
    let ticks = (rect.width() / (zoom * 5.0)).ceil() as usize;
    for index in 0..=ticks {
        let x = rect.left() + index as f32 * zoom * 5.0;
        painter.line_segment(
            [
                Pos2::new(x, rect.top() + RULER_HEIGHT),
                Pos2::new(x, rect.bottom()),
            ],
            Stroke::new(1.0_f32, theme::BORDER.gamma_multiply(0.28)),
        );
    }
    let _ = project;
}

fn paint_ruler(ui: &mut Ui, rect: Rect, zoom: f32, duration: f64, playhead: f64) {
    let painter = ui.painter();
    let interval = choose_tick_interval(zoom);
    let minor = interval / 4.0;
    let mut t = 0.0_f64;
    while t <= duration + interval {
        let x = rect.left() + t as f32 * zoom;
        if x > rect.right() {
            break;
        }
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom() - 7.0),
                Pos2::new(x, rect.bottom()),
            ],
            Stroke::new(1.0_f32, theme::MUTED.gamma_multiply(0.7)),
        );
        painter.text(
            Pos2::new(x + 4.0, rect.top() + 6.0),
            Align2::LEFT_TOP,
            format_ruler_time(t),
            FontId::monospace(9.5),
            theme::MUTED,
        );
        t += interval;
    }
    let mut mt = 0.0_f64;
    while mt <= duration + minor {
        let x = rect.left() + mt as f32 * zoom;
        if x > rect.right() {
            break;
        }
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom() - 3.0),
                Pos2::new(x, rect.bottom()),
            ],
            Stroke::new(1.0_f32, theme::BORDER),
        );
        mt += minor;
    }
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, theme::BORDER),
    );
    let _ = playhead;
}

fn choose_tick_interval(zoom: f32) -> f64 {
    for candidate in [0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0] {
        if candidate as f32 * zoom >= 62.0 {
            return candidate;
        }
    }
    120.0
}

fn format_ruler_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn paint_timeline_clip(
    ui: &mut Ui,
    rect: Rect,
    clip: &Clip,
    track: &Track,
    missing: bool,
    active: bool,
    selected: bool,
) {
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let mut fill = clip_color(track, clip);
    if missing {
        fill = Color32::from_rgb(64, 30, 38);
    }
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    let stroke = if active || selected {
        Stroke::new(2.0_f32, theme::ACCENT)
    } else if missing {
        Stroke::new(1.5_f32, theme::DANGER)
    } else {
        Stroke::new(1.0_f32, fill.gamma_multiply(1.5))
    };
    painter.rect_stroke(rect, CornerRadius::same(5), stroke, StrokeKind::Inside);
    if clip.title.is_some() {
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            CornerRadius::same(2),
            theme::GOLD,
        );
    }

    let label = if clip.name.trim().is_empty() {
        "Untitled"
    } else {
        &clip.name
    };
    painter.text(
        Pos2::new(rect.left() + 9.0, rect.top() + 5.0),
        Align2::LEFT_TOP,
        truncate(label, ((rect.width() / 7.0) as usize).clamp(4, 34)),
        FontId::proportional(11.5),
        theme::TEXT,
    );
    let mut badges: Vec<String> = Vec::new();
    if missing {
        badges.push("OFFLINE".into());
    }
    if clip.title.is_some() {
        badges.push("TITLE".into());
    }
    if !clip.effects.is_empty() {
        badges.push(format!("{} FX", clip.effects.len()));
    }
    if clip.audio.muted {
        badges.push("MUTE".into());
    } else if track.kind == TrackKind::Audio && clip.audio.volume < 0.999 {
        badges.push(format!("VOL {:.0}%", clip.audio.volume * 100.0));
    }
    let badge_text = badges.join("  ·  ");
    if !badge_text.is_empty() {
        painter.text(
            Pos2::new(rect.left() + 9.0, rect.top() + 21.0),
            Align2::LEFT_TOP,
            truncate(&badge_text, ((rect.width() / 5.5) as usize).clamp(4, 30)),
            FontId::proportional(9.0),
            if missing { theme::DANGER } else { theme::MUTED },
        );
    }
    if rect.width() > 54.0 {
        painter.text(
            Pos2::new(rect.right() - 7.0, rect.top() + 5.0),
            Align2::RIGHT_TOP,
            format!("{:.1}s", clip.duration()),
            FontId::monospace(9.0),
            Color32::from_white_alpha(150),
        );
    }
    if clip.title.is_some() && rect.width() > 110.0 {
        painter.text(
            Pos2::new(rect.left() + 9.0, rect.bottom() - 14.0),
            Align2::LEFT_TOP,
            "double-click to seek",
            FontId::proportional(8.5),
            Color32::from_white_alpha(80),
        );
    }
}

fn paint_transition_marker(ui: &Ui, rect: Rect, clip: &Clip, zoom: f32) {
    let Some(transition) = &clip.transition else {
        return;
    };
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let width = (transition.duration as f32 * zoom).clamp(2.0, rect.width() * 0.5);
    let band = Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
    painter.rect_filled(
        band,
        CornerRadius::same(4),
        theme::ACCENT.gamma_multiply(0.22),
    );
    painter.line_segment(
        [
            Pos2::new(band.left() + 1.0, rect.top() + 3.0),
            Pos2::new(band.left() + 1.0, rect.bottom() - 3.0),
        ],
        Stroke::new(1.0_f32, theme::ACCENT.gamma_multiply(0.8)),
    );
}

fn paint_audio_fades(ui: &Ui, rect: Rect, clip: &Clip) {
    if clip.audio.fade_in <= 0.001 && clip.audio.fade_out <= 0.001 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let duration = clip.duration().max(0.01);
    let stroke = Stroke::new(1.2_f32, Color32::from_white_alpha(110));
    if clip.audio.fade_in > 0.001 {
        let x = rect.left() + (clip.audio.fade_in.min(duration) / duration) as f32 * rect.width();
        painter.line_segment([rect.left_bottom(), Pos2::new(x, rect.top())], stroke);
    }
    if clip.audio.fade_out > 0.001 {
        let x = rect.right() - (clip.audio.fade_out.min(duration) / duration) as f32 * rect.width();
        painter.line_segment([Pos2::new(x, rect.top()), rect.right_bottom()], stroke);
    }
}

fn paint_trim_handle(ui: &Ui, rect: Rect, left: bool) {
    let painter = ui.painter();
    let x = if left {
        rect.left() + 2.0
    } else {
        rect.right() - 2.0
    };
    painter.line_segment(
        [
            Pos2::new(x, rect.top() + 10.0),
            Pos2::new(x, rect.bottom() - 10.0),
        ],
        Stroke::new(1.0_f32, Color32::from_white_alpha(70)),
    );
}

fn snap_time(value: f64, snap: bool) -> f64 {
    if snap {
        (value * 10.0).round() / 10.0
    } else {
        value
    }
}

fn round_time(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut out: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}
