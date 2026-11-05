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
