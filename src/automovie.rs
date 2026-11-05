use anyhow::{bail, Result};

use crate::edit::reflow_primary_ordered;
use crate::model::{
    default_clip_for, new_id, AudioParams, Project, Title, Transition, TransitionKind,
};

#[derive(Debug, Clone)]
pub struct AutoMovieOptions {
    pub opening_title: bool,
    pub opening_text: String,
    pub opening_duration: f64,
    pub closing_credits: bool,
    pub closing_text: String,
    pub closing_subtitle: String,
    pub closing_duration: f64,
    pub transition: TransitionKind,
    pub transition_duration: f64,
    pub fit_to_music: bool,
    pub music_media_id: Option<String>,
}

impl Default for AutoMovieOptions {
    fn default() -> Self {
        Self {
            opening_title: true,
            opening_text: String::new(),
            opening_duration: 3.0,
            closing_credits: true,
            closing_text: "THE END".to_string(),
            closing_subtitle: "Edited with Splice".to_string(),
            closing_duration: 3.0,
            transition: TransitionKind::Fade,
            transition_duration: 0.7,
            fit_to_music: true,
            music_media_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AutoMovieReport {
    pub clips: usize,
    pub primary_duration: f64,
    pub total_duration: f64,
    pub music: Option<String>,
}

/// Turns the current primary sequence into a Movie Maker style auto-movie:
/// opening title, crossfaded scene changes, a closing credit roll and, when a
/// music asset is chosen, a sequence fitted to that track.
pub fn build(project: &mut Project, options: &AutoMovieOptions) -> Result<AutoMovieReport> {
    let clip_count = project
        .primary_video_track()
        .map(|track| track.clips.len())
        .unwrap_or(0);
    if clip_count == 0 {
        bail!("add video or image clips to the timeline before running AutoMovie");
    }

    let music_asset = match options.music_media_id.as_deref() {
        Some(id) => {
            let asset = project
                .media_by_id(id)
                .ok_or_else(|| anyhow::anyhow!("selected music asset no longer exists"))?;
            if asset.kind != crate::model::MediaKind::Audio {
                bail!("AutoMovie music must be an audio asset");
            }
            Some(asset.clone())
        }
        None => None,
    };

    let transition = options.transition;
    let requested_duration = options.transition_duration.clamp(0.15, 2.0);
    let target_duration = if options.fit_to_music {
        music_asset.as_ref().map(|asset| asset.duration.max(1.0))
    } else {
        None
    };

    // Establish transitions first so the fit calculation knows their overlap.
    {
        let track = project
            .primary_video_track_mut()
            .ok_or_else(|| anyhow::anyhow!("project has no primary video track"))?;
        track.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
        for (index, clip) in track.clips.iter_mut().enumerate() {
            if index == 0 {
                clip.transition = None;
            } else {
                clip.transition = Some(Transition {
                    kind: transition,
                    duration: requested_duration.min(clip.duration() * 0.45).max(0.05),
                });
            }
        }
    }

    if let Some(target) = target_duration {
        let mut total_base = 0.0_f64;
        let mut total_transition = 0.0_f64;
        {
            let track = project.primary_video_track().expect("track checked");
            for (index, clip) in track.clips.iter().enumerate() {
                total_base += clip.duration().max(0.1);
                if index > 0 {
                    total_transition += clip
                        .transition
                        .as_ref()
                        .map(|transition| transition.duration)
                        .unwrap_or(0.0);
                }
            }
        }
        let scale = (target + total_transition) / total_base.max(0.1);
        {
            let track = project.primary_video_track_mut().expect("track checked");
            for clip in &mut track.clips {
                let base = clip.duration().max(0.1);
                let scaled = (base * scale).max(0.35);
                clip.out_point = clip.in_point + scaled;
            }
        }
        // Keep overlaps physically possible after scaling.
        if let Some(track) = project.primary_video_track_mut() {
            for i in 1..track.clips.len() {
                let previous = track.clips[i - 1].duration();
                let current = track.clips[i].duration();
                if let Some(transition) = &mut track.clips[i].transition {
                    transition.duration = transition
                        .duration
                        .min(previous * 0.45)
                        .min(current * 0.45)
                        .max(0.05);
                }
            }
        }
    }
    reflow_primary_ordered(project);

    // Remove previous auto-generated titles so repeated runs do not stack.
    let opening_name = if options.opening_text.trim().is_empty() {
        project.name.clone()
    } else {
        options.opening_text.trim().to_string()
    };
    let closing_text = options.closing_text.trim().to_string();
    for track in &mut project.tracks {
        track.clips.retain(|clip| {
            let Some(title) = &clip.title else {
                return true;
            };
            !(title.text == opening_name || title.text == closing_text || title.text == "THE END")
        });
    }

    if options.opening_title {
        let mut title = Title::opening(&project.name);
        title.text = opening_name.clone();
        let duration = options.opening_duration.clamp(0.5, 15.0);
        project.add_title(title, 0.0, duration);
    }

    let primary_duration = project.primary_end();
    if options.closing_credits {
        let mut credits = Title::credits();
        credits.text = closing_text;
        credits.subtitle = options.closing_subtitle.clone();
        let duration = options.closing_duration.clamp(0.5, 20.0);
        project.add_title(credits, primary_duration, duration);
    }

    let total_duration = project.timeline_duration();

    let music_name = if let Some(asset) = &music_asset {
        // Replace the music track with the chosen track, trimmed to the movie.
        let track = project
            .tracks
            .iter()
            .position(|track| track.id == "track-audio-music")
            .or_else(|| {
                project
                    .tracks
                    .iter()
                    .position(|track| track.kind == crate::model::TrackKind::Audio)
            });
        if let Some(index) = track {
            let track = &mut project.tracks[index];
            track.clips.clear();
            track.muted = false;
            let mut clip = default_clip_for(asset, 0.0);
            clip.id = new_id("clip");
            clip.out_point = if options.fit_to_music {
                primary_duration.min(asset.duration)
            } else {
                asset.duration
            };
            if clip.out_point <= 0.05 {
                clip.out_point = asset.duration.max(0.1);
            }
            clip.audio = AudioParams {
                volume: 0.8,
                fade_in: 0.8_f64.min(clip.out_point * 0.2),
                fade_out: 1.6_f64.min(clip.out_point * 0.25),
                muted: false,
            };
            track.clips.push(clip);
        }
        Some(asset.name.clone())
    } else {
        None
    };

    Ok(AutoMovieReport {
        clips: clip_count,
        primary_duration,
        total_duration,
        music: music_name,
    })
}
