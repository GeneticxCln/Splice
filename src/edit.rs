use anyhow::{Result, bail};

use crate::model::{
    Clip, MediaAsset, MediaKind, Project, Title, TrackKind, default_clip_for, new_id,
};

/// Sequential reflow that preserves the vector order rather than sorting by
/// start. Storyboard ordering and duplication both depend on explicit order.
pub fn reflow_primary_ordered(project: &mut Project) {
    let Some(track) = project.primary_video_track_mut() else {
        return;
    };
    let mut cursor = 0.0_f64;
    for (index, clip) in track.clips.iter_mut().enumerate() {
        let overlap = clip
            .transition
            .as_ref()
            .map(|transition| transition.duration.max(0.0))
            .unwrap_or(0.0);
        if index == 0 {
            clip.start = 0.0;
        } else {
            let overlap = overlap.min(cursor * 0.5).min(clip.duration() * 0.5);
            clip.start = (cursor - overlap).max(0.0);
        }
        cursor = clip.start + clip.duration();
    }
}

pub fn add_asset_to_timeline(project: &mut Project, asset: &MediaAsset) -> Result<Option<String>> {
    match asset.kind {
        MediaKind::Video | MediaKind::Image => {
            let clip = default_clip_for(asset, 0.0);
            let id = clip.id.clone();
            let track = project
                .primary_video_track_mut()
                .ok_or_else(|| anyhow::anyhow!("project has no primary video track"))?;
            track.clips.push(clip);
            reflow_primary_ordered(project);
            Ok(Some(id))
        }
        MediaKind::Audio => {
            let mut clip = default_clip_for(asset, 0.0);
            clip.name = asset.name.clone();
            let id = clip.id.clone();
            let track = project
                .first_audio_track_mut()
                .ok_or_else(|| anyhow::anyhow!("project has no audio track"))?;
            track.clips.push(clip);
            Ok(Some(id))
        }
    }
}

pub fn add_asset_to_voiceover(
    project: &mut Project,
    asset: &MediaAsset,
    start: f64,
) -> Result<String> {
    let mut clip = default_clip_for(asset, start.max(0.0));
    clip.name = format!("Voiceover — {}", asset.name);
    let id = clip.id.clone();
    let index = project
        .tracks
        .iter()
        .position(|track| track.id == "track-audio-voice" && track.kind == TrackKind::Audio)
        .or_else(|| {
            project
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, track)| track.kind == TrackKind::Audio)
                .nth(1)
                .map(|(index, _)| index)
        })
        .ok_or_else(|| anyhow::anyhow!("project has no voiceover track"))?;
    project.tracks[index].clips.push(clip);
    Ok(id)
}

pub fn add_asset_to_overlay(
    project: &mut Project,
    asset: &MediaAsset,
    start: f64,
) -> Result<String> {
    let clip = default_clip_for(asset, start.max(0.0));
    let id = clip.id.clone();
    let track = project
        .overlay_track_mut()
        .ok_or_else(|| anyhow::anyhow!("project has no overlay track"))?;
    track.clips.push(clip);
    Ok(id)
}

pub fn add_title_clip(project: &mut Project, title: Title, start: f64, duration: f64) -> String {
    project.add_title(title, start.max(0.0), duration.max(0.25))
}

pub fn split_clip(project: &mut Project, clip_id: &str, at: f64) -> Result<Option<String>> {
    let mut found: Option<(usize, usize, Clip)> = None;
    for (track_index, track) in project.tracks.iter().enumerate() {
        if let Some(clip_index) = track.clips.iter().position(|clip| clip.id == clip_id) {
            found = Some((track_index, clip_index, track.clips[clip_index].clone()));
            break;
        }
    }
    let Some((track_index, clip_index, original)) = found else {
        bail!("clip {clip_id} no longer exists");
    };
    let offset = at - original.start;
    if offset <= 0.08 || offset >= original.duration() - 0.08 {
        bail!("playhead is not inside the clip; move it away from the edges");
    }

    let (left_motion, right_motion) = original.motion.split(offset);
    let mut left = original.clone();
    left.out_point = left.in_point + offset;
    left.motion = left_motion;
    let mut right = original;
    right.id = new_id("clip");
    right.name = format!("{} (split)", right.name);
    right.start = at;
    right.in_point += offset;
    right.transition = None;
    right.motion = right_motion;
    for effect in &mut right.effects {
        effect.id = new_id("fx");
    }
    let right_id = right.id.clone();

    let track = &mut project.tracks[track_index];
    track.clips[clip_index] = left;
    track.clips.insert(clip_index + 1, right);
    Ok(Some(right_id))
}

pub fn duplicate_clip(project: &mut Project, clip_id: &str) -> Result<Option<String>> {
    let mut found: Option<(usize, usize, Clip)> = None;
    for (track_index, track) in project.tracks.iter().enumerate() {
        if let Some(clip_index) = track.clips.iter().position(|clip| clip.id == clip_id) {
            found = Some((track_index, clip_index, track.clips[clip_index].clone()));
            break;
        }
    }
    let Some((track_index, clip_index, original)) = found else {
        bail!("clip {clip_id} no longer exists");
    };
    let mut duplicate = original.clone();
    duplicate.id = new_id("clip");
    duplicate.name = format!("{} copy", duplicate.name);
    for effect in &mut duplicate.effects {
        effect.id = new_id("fx");
    }

    let track_kind = project.tracks[track_index].kind;
    if track_kind == TrackKind::Video {
        duplicate.start = original.start + original.duration();
        duplicate.transition = None;
        project.tracks[track_index]
            .clips
            .insert(clip_index + 1, duplicate);
        reflow_primary_ordered(project);
    } else {
        duplicate.start = original.start + original.duration();
        project.tracks[track_index]
            .clips
            .insert(clip_index + 1, duplicate);
    }
    Ok(Some(
        project.tracks[track_index].clips[clip_index + 1].id.clone(),
    ))
}

pub fn delete_clip(project: &mut Project, clip_id: &str) -> bool {
    let mut removed_kind = None;
    for track in &mut project.tracks {
        if let Some(index) = track.clips.iter().position(|clip| clip.id == clip_id) {
            track.clips.remove(index);
            removed_kind = Some(track.kind);
            break;
        }
    }
    if removed_kind == Some(TrackKind::Video) {
        reflow_primary_ordered(project);
    }
    removed_kind.is_some()
}

pub fn move_primary_clip_to_index(
    project: &mut Project,
    clip_id: &str,
    target_index: usize,
) -> bool {
    let Some(track) = project.primary_video_track_mut() else {
        return false;
    };
    let Some(index) = track.clips.iter().position(|clip| clip.id == clip_id) else {
        return false;
    };
    let clip = track.clips.remove(index);
    let target = target_index.min(track.clips.len());
    track.clips.insert(target, clip);
    reflow_primary_ordered(project);
    true
}

pub fn move_primary_clip_by(project: &mut Project, clip_id: &str, delta: isize) -> bool {
    let Some(track) = project.primary_video_track() else {
        return false;
    };
    let Some(index) = track.clips.iter().position(|clip| clip.id == clip_id) else {
        return false;
    };
    let target = (index as isize + delta).clamp(0, track.clips.len() as isize - 1) as usize;
    if target == index {
        return false;
    }
    move_primary_clip_to_index(project, clip_id, target)
}

pub fn sort_and_reflow_primary(project: &mut Project) {
    let Some(track) = project.primary_video_track_mut() else {
        return;
    };
    track.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
    reflow_primary_ordered(project);
}

pub fn clip_track_kind(project: &Project, clip_id: &str) -> Option<TrackKind> {
    project.clip_by_id(clip_id).map(|(track, _)| track.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MediaAsset, Transition, TransitionKind};

    fn asset(kind: MediaKind, duration: f64) -> MediaAsset {
        MediaAsset {
            id: format!("media-{duration}"),
            name: format!("m{duration}"),
            path: "x.mp4".into(),
            kind,
            duration,
            width: Some(640),
            height: Some(360),
            has_audio: true,
            sample_rate: Some(48_000),
            channels: Some(2),
        }
    }

    fn project_with_clips(durations: &[f64]) -> (Project, Vec<String>) {
        let mut project = Project::new("t");
        let mut ids = Vec::new();
        for duration in durations {
            let asset = asset(MediaKind::Video, *duration);
            project.media.push(asset.clone());
            ids.push(
                add_asset_to_timeline(&mut project, &asset)
                    .unwrap()
                    .unwrap(),
            );
        }
        (project, ids)
    }

    fn primary_ids(project: &Project) -> Vec<String> {
        project
            .primary_video_track()
            .unwrap()
            .clips
            .iter()
            .map(|clip| clip.id.clone())
            .collect()
    }

    fn starts(project: &Project) -> Vec<f64> {
        project
            .primary_video_track()
            .unwrap()
            .clips
            .iter()
            .map(|clip| clip.start)
            .collect()
    }

    #[test]
    fn adding_clips_lays_them_end_to_end() {
        let (project, _) = project_with_clips(&[2.0, 3.0, 1.5]);
        assert_eq!(starts(&project), vec![0.0, 2.0, 5.0]);
        assert!((project.primary_end() - 6.5).abs() < 1e-9);
    }

    #[test]
    fn transitions_overlap_the_previous_clip() {
        let (mut project, ids) = project_with_clips(&[4.0, 4.0]);
        project.clip_by_id_mut(&ids[1]).unwrap().1.transition = Some(Transition {
            kind: TransitionKind::Fade,
            duration: 1.0,
        });
        reflow_primary_ordered(&mut project);
        assert_eq!(starts(&project), vec![0.0, 3.0]);
        // The overlap can never exceed half of either clip.
        project
            .clip_by_id_mut(&ids[1])
            .unwrap()
            .1
            .transition
            .as_mut()
            .unwrap()
            .duration = 30.0;
        reflow_primary_ordered(&mut project);
        assert_eq!(starts(&project), vec![0.0, 2.0]);
    }

    #[test]
    fn split_divides_a_clip_without_losing_time() {
        let (mut project, ids) = project_with_clips(&[4.0]);
        let right = split_clip(&mut project, &ids[0], 1.5).unwrap().unwrap();
        let clips = &project.primary_video_track().unwrap().clips;
        assert_eq!(clips.len(), 2);
        assert_eq!(clips[0].id, ids[0]);
        assert_eq!(clips[1].id, right);
        assert!((clips[0].duration() - 1.5).abs() < 1e-9);
        assert!((clips[1].duration() - 2.5).abs() < 1e-9);
        assert!((clips[1].in_point - 1.5).abs() < 1e-9);
        assert!((clips[1].start - 1.5).abs() < 1e-9);
        assert!(clips[1].transition.is_none());
    }

    #[test]
    fn split_rejects_edges_and_unknown_clips() {
        let (mut project, ids) = project_with_clips(&[4.0]);
        let before = project.clone();
        for at in [0.0, 0.05, 3.95, 4.0, 9.0] {
            assert!(split_clip(&mut project, &ids[0], at).is_err(), "at {at}");
        }
        assert!(split_clip(&mut project, "missing", 1.0).is_err());
        assert_eq!(project, before, "a rejected split must not change anything");
    }

    #[test]
    fn duplicate_gets_new_ids_and_follows_the_original() {
        let (mut project, ids) = project_with_clips(&[2.0, 2.0]);
        let copy = duplicate_clip(&mut project, &ids[0]).unwrap().unwrap();
        let order = primary_ids(&project);
        assert_eq!(order, vec![ids[0].clone(), copy.clone(), ids[1].clone()]);
        assert_eq!(starts(&project), vec![0.0, 2.0, 4.0]);
        let unique: std::collections::HashSet<_> = order.iter().collect();
        assert_eq!(unique.len(), 3);
        assert!(duplicate_clip(&mut project, "missing").is_err());
    }

    #[test]
    fn delete_closes_the_gap_on_the_primary_track() {
        let (mut project, ids) = project_with_clips(&[2.0, 3.0, 1.0]);
        assert!(delete_clip(&mut project, &ids[1]));
        assert_eq!(starts(&project), vec![0.0, 2.0]);
        assert!(!delete_clip(&mut project, &ids[1]), "already gone");
    }

    #[test]
    fn reordering_keeps_clips_contiguous() {
        let (mut project, ids) = project_with_clips(&[1.0, 2.0, 3.0]);
        assert!(move_primary_clip_to_index(&mut project, &ids[2], 0));
        assert_eq!(
            primary_ids(&project),
            vec![ids[2].clone(), ids[0].clone(), ids[1].clone()]
        );
        assert_eq!(starts(&project), vec![0.0, 3.0, 4.0]);
        assert!(move_primary_clip_by(&mut project, &ids[2], 1));
        assert_eq!(primary_ids(&project)[1], ids[2]);
        assert!(
            !move_primary_clip_by(&mut project, &ids[1], 5),
            "already last"
        );
        assert!(!move_primary_clip_to_index(&mut project, "missing", 0));
    }

    #[test]
    fn audio_and_overlay_clips_use_their_own_tracks() {
        let mut project = Project::new("t");
        let music = asset(MediaKind::Audio, 10.0);
        let video = asset(MediaKind::Video, 4.0);
        add_asset_to_timeline(&mut project, &music).unwrap();
        let voice = add_asset_to_voiceover(&mut project, &music, 2.0).unwrap();
        let overlay = add_asset_to_overlay(&mut project, &video, 1.0).unwrap();

        let track_of = |id: &str| project.clip_by_id(id).unwrap().0.id.clone();
        assert_eq!(track_of(&voice), "track-audio-voice");
        assert_eq!(track_of(&overlay), "track-overlay-1");
        assert_eq!(
            project.clip_by_id(&voice).unwrap().1.start,
            2.0,
            "voiceover starts at the playhead"
        );
        assert!(project.primary_video_track().unwrap().clips.is_empty());
    }

    #[test]
    fn splitting_a_title_keeps_both_halves_titles() {
        let mut project = Project::new("t");
        let id = add_title_clip(&mut project, Title::opening("Hi"), 1.0, 4.0);
        let right = split_clip(&mut project, &id, 3.0).unwrap().unwrap();
        assert!(project.clip_by_id(&id).unwrap().1.title.is_some());
        assert!(project.clip_by_id(&right).unwrap().1.title.is_some());
        assert_eq!(project.title_clips().len(), 2);
    }
}
