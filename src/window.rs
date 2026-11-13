//! Windowed preview for long timelines.
//!
//! Rendering the whole timeline after every edit does not scale: a ten minute
//! project takes far too long to preview. Past [`FULL_RENDER_LIMIT`] the
//! preview renders only a [`WINDOW_LENGTH`] second slice around the playhead,
//! built by cutting a copy of the project down to that slice.

use crate::model::{Clip, Project};

/// Timelines up to this long are previewed in one piece.
pub const FULL_RENDER_LIMIT: f64 = 60.0;
/// Length of one preview slice of a long timeline.
pub const WINDOW_LENGTH: f64 = 30.0;
/// How far before the playhead a slice starts, so scrubbing back a little
/// does not need a new render.
pub const WINDOW_LEAD_IN: f64 = 5.0;

const EDGE: f64 = 0.001;

/// The slice `(start, end)` of the timeline to preview around `playhead`, or
/// `None` when the whole timeline is short enough to render at once.
pub fn choose_window(playhead: f64, total: f64) -> Option<(f64, f64)> {
    if !total.is_finite() || total <= FULL_RENDER_LIMIT {
        return None;
    }
    let mut start = (playhead - WINDOW_LEAD_IN).floor().max(0.0);
    if start + WINDOW_LENGTH > total {
        start = (total - WINDOW_LENGTH).floor().max(0.0);
    }
    Some((start, (start + WINDOW_LENGTH).min(total)))
}

/// Whether a proxy rendered for `window` can show `time`. A slice that ends at
/// the end of the timeline can show its last frame; any other slice cannot show
/// its own end, which belongs to the next slice.
pub fn window_contains(window: Option<(f64, f64)>, time: f64, total: f64) -> bool {
    match window {
        None => true,
        Some((start, end)) => time >= start - EDGE && (time < end - EDGE || end >= total - 0.05),
    }
}

/// A copy of `project` holding only what plays between `start` and `end`,
/// shifted so `start` becomes time zero. Clips that straddle an edge are cut
/// the same way a split would cut them, and the fades that would restart at an
/// artificial cut are dropped.
pub fn project_window(project: &Project, start: f64, end: f64) -> Project {
    let mut window = project.clone();
    for track in &mut window.tracks {
        let clips = std::mem::take(&mut track.clips);
        track.clips = clips
            .into_iter()
            .filter_map(|clip| cut_clip(clip, start, end))
            .collect();
    }
    window
}

fn cut_clip(mut clip: Clip, start: f64, end: f64) -> Option<Clip> {
    let clip_start = clip.start;
    let clip_end = clip_start + clip.duration();
    if clip_end <= start + EDGE || clip_start >= end - EDGE {
        return None;
    }
    if clip_start < start {
        let offset = start - clip_start;
        clip.in_point += offset;
        clip.motion = clip.motion.split(offset).1;
        clip.transition = None;
        clip.audio.fade_in = 0.0;
        clip.start = 0.0;
    } else {
        clip.start = clip_start - start;
    }
    if clip_end > end {
        let visible = end - clip_start.max(start);
        clip.out_point = clip.in_point + visible;
        clip.audio.fade_out = 0.0;
    }
    Some(clip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{add_asset_to_timeline, add_title_clip};
    use crate::model::{
        AudioParams, ClipMotion, MediaAsset, MediaKind, MotionEasing, MotionKeyframe, Title,
        Transition, TransitionKind,
    };

    #[test]
    fn short_timelines_are_never_windowed() {
        assert_eq!(choose_window(10.0, 60.0), None);
        assert_eq!(choose_window(0.0, 12.0), None);
        assert_eq!(choose_window(0.0, f64::NAN), None);
    }

    #[test]
    fn windows_lead_the_playhead_and_stay_inside_the_timeline() {
        assert_eq!(choose_window(0.0, 600.0), Some((0.0, 30.0)));
        assert_eq!(choose_window(100.0, 600.0), Some((95.0, 125.0)));
        assert_eq!(choose_window(3.0, 600.0), Some((0.0, 30.0)));
        // Near the end the window backs up so it stays full length.
        assert_eq!(choose_window(599.0, 600.0), Some((570.0, 600.0)));
        assert_eq!(choose_window(70.0, 75.0), Some((45.0, 75.0)));
    }

    #[test]
    fn a_window_contains_its_span_but_hands_its_end_to_the_next_one() {
        let window = Some((95.0, 125.0));
        assert!(window_contains(window, 95.0, 600.0));
        assert!(window_contains(window, 110.0, 600.0));
        assert!(!window_contains(window, 94.0, 600.0));
        assert!(!window_contains(window, 125.0, 600.0));
        assert!(window_contains(Some((570.0, 600.0)), 600.0, 600.0));
        assert!(window_contains(None, 1e9, 600.0));
    }

    fn asset(duration: f64) -> MediaAsset {
        MediaAsset {
            id: "m".into(),
            name: "m".into(),
            path: "x.mp4".into(),
            kind: MediaKind::Video,
            duration,
            width: Some(640),
            height: Some(360),
            has_audio: true,
            sample_rate: Some(48_000),
            channels: Some(2),
        }
    }

    fn long_project() -> (Project, Vec<String>) {
        let mut project = Project::new("long");
        let asset = asset(10.0);
        project.media.push(asset.clone());
        let mut ids = Vec::new();
        for _ in 0..4 {
            ids.push(
                add_asset_to_timeline(&mut project, &asset)
                    .unwrap()
                    .unwrap(),
            );
        }
        (project, ids)
    }

    fn primary(project: &Project) -> &[Clip] {
        &project.primary_video_track().unwrap().clips
    }

    #[test]
    fn clips_are_cut_at_the_window_edges_and_shifted_to_zero() {
        let (project, ids) = long_project(); // clips at 0,10,20,30
        let window = project_window(&project, 15.0, 32.0);
        let clips = primary(&window);
        assert_eq!(clips.len(), 3);
        // Cut on the left: starts the window, skips 5 s into its source.
        assert_eq!(clips[0].id, ids[1]);
        assert_eq!(clips[0].start, 0.0);
        assert!((clips[0].in_point - 5.0).abs() < 1e-9);
        assert!((clips[0].duration() - 5.0).abs() < 1e-9);
        // Whole clip, shifted.
        assert_eq!(clips[1].start, 5.0);
        assert!((clips[1].duration() - 10.0).abs() < 1e-9);
        // Cut on the right: only 2 s remain.
        assert_eq!(clips[2].start, 15.0);
        assert!((clips[2].duration() - 2.0).abs() < 1e-9);
        assert!((window.timeline_duration() - 17.0).abs() < 1e-9);
        // The original is untouched.
        assert_eq!(primary(&project).len(), 4);
    }

    #[test]
    fn clips_outside_the_window_are_dropped_on_every_track() {
        let (mut project, _) = long_project();
        let title_in = add_title_clip(&mut project, Title::opening("in"), 20.0, 3.0);
        let title_out = add_title_clip(&mut project, Title::opening("out"), 1.0, 3.0);
        let window = project_window(&project, 18.0, 28.0);
        assert!(window.clip_by_id(&title_in).is_some());
        assert!(window.clip_by_id(&title_out).is_none());
        assert_eq!(window.clip_by_id(&title_in).unwrap().1.start, 2.0);
    }

    #[test]
    fn artificial_cuts_drop_fades_and_transitions_but_real_ones_stay() {
        let (mut project, ids) = long_project();
        for id in &ids {
            let clip = &mut project.clip_by_id_mut(id).unwrap().1;
            clip.audio = AudioParams {
                volume: 1.0,
                fade_in: 1.0,
                fade_out: 1.0,
                muted: false,
            };
        }
        project.clip_by_id_mut(&ids[1]).unwrap().1.transition = Some(Transition {
            kind: TransitionKind::Fade,
            duration: 1.0,
        });
        project.clip_by_id_mut(&ids[2]).unwrap().1.transition = Some(Transition {
            kind: TransitionKind::Fade,
            duration: 1.0,
        });
        let window = project_window(&project, 15.0, 25.0);
        let clips = primary(&window);
        // ids[1] is cut on the left, ids[2] is cut on the right.
        assert!(clips[0].transition.is_none());
        assert_eq!(clips[0].audio.fade_in, 0.0);
        assert_eq!(clips[0].audio.fade_out, 1.0, "its real end is inside");
        assert!(clips[1].transition.is_some(), "a real transition stays");
        assert_eq!(clips[1].audio.fade_in, 1.0);
        assert_eq!(clips[1].audio.fade_out, 0.0, "cut short by the window");
    }

    #[test]
    fn motion_keeps_its_curve_across_a_left_cut() {
        let (mut project, ids) = long_project();
        project.clip_by_id_mut(&ids[0]).unwrap().1.motion = ClipMotion {
            keyframes: vec![
                MotionKeyframe {
                    time: 0.0,
                    scale: 1.0,
                    pan_x: 0.0,
                    pan_y: 0.0,
                    rotation: 0.0,
                    opacity: 1.0,
                },
                MotionKeyframe {
                    time: 10.0,
                    scale: 2.0,
                    pan_x: 0.0,
                    pan_y: 0.0,
                    rotation: 0.0,
                    opacity: 1.0,
                },
            ],
            easing: MotionEasing::Linear,
            origin: 0.0,
        };
        let original = project.clip_by_id(&ids[0]).unwrap().1.motion.clone();
        let window = project_window(&project, 4.0, 40.0);
        let cut = &window.clip_by_id(&ids[0]).unwrap().1.motion;
        for local in [0.0, 1.0, 3.0, 5.5] {
            let expected = original.sample(local + 4.0);
            let actual = cut.sample(local);
            assert!((expected.scale - actual.scale).abs() < 1e-9, "at {local}");
        }
    }
}
