use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::ffmpeg::{ProbeInfo, Toolchain};
use crate::model::{MediaAsset, MediaKind, Project, SAMPLE_RATE, new_id, stored_media_path};

/// Probes a real file through system ffprobe and appends it to the media
/// library. The source file is only read.
pub fn import_media(
    project: &mut Project,
    project_dir: Option<&Path>,
    tools: &Toolchain,
    path: &Path,
) -> Result<MediaAsset> {
    let asset = probe_asset(project_dir, tools, path)?;
    project.media.push(asset.clone());
    Ok(asset)
}

/// Probes `path` and builds the library entry without touching any project,
/// so it can run on a worker thread. With `project_dir` the stored path is
/// relative to it where possible; without, it is absolute.
pub fn probe_asset(
    project_dir: Option<&Path>,
    tools: &Toolchain,
    path: &Path,
) -> Result<MediaAsset> {
    let absolute = std::fs::canonicalize(path)
        .with_context(|| format!("could not resolve media path {}", path.display()))?;
    let info = tools.probe(&absolute)?;
    if info.kind != MediaKind::Image && info.duration <= 0.05 {
        bail!(
            "{} has no usable duration (probed {:.3}s)",
            absolute.display(),
            info.duration
        );
    }

    let name = absolute
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Media")
        .to_string();
    let stored = match project_dir {
        Some(directory) => stored_media_path(directory, &absolute),
        None => absolute.to_string_lossy().replace('\\', "/"),
    };

    Ok(MediaAsset {
        id: new_id("media"),
        name,
        path: stored,
        kind: info.kind,
        duration: info.duration,
        width: info.width,
        height: info.height,
        has_audio: info.has_audio,
        sample_rate: info.sample_rate.or(if info.kind == MediaKind::Audio {
            Some(SAMPLE_RATE)
        } else {
            None
        }),
        channels: info.channels,
    })
}

/// Validates + applies a relink using ffprobe. Returns the refreshed probe data
/// so callers can report exactly what the new source contains.
pub fn relink_media(
    project: &mut Project,
    project_dir: &Path,
    tools: &Toolchain,
    media_id: &str,
    new_path: &Path,
) -> Result<ProbeInfo> {
    let info = tools.probe(new_path)?;
    let canonical = std::fs::canonicalize(new_path)
        .with_context(|| format!("could not resolve relink path {}", new_path.display()))?;
    let stored = stored_media_path(project_dir, &canonical);

    let Some(asset) = project.media.iter_mut().find(|asset| asset.id == media_id) else {
        bail!("unknown media id {media_id}");
    };

    asset.path = stored;
    asset.kind = info.kind;
    asset.duration = info.duration;
    asset.width = info.width;
    asset.height = info.height;
    asset.has_audio = info.has_audio;
    asset.sample_rate = info.sample_rate;
    asset.channels = info.channels;
    Ok(info)
}

pub fn probe_or_offline(tools: &Toolchain, path: &Path) -> Option<ProbeInfo> {
    tools.probe(path).ok()
}
