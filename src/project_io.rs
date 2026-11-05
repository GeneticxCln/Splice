use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use thiserror::Error;

use crate::model::{
    PROJECT_EXTENSION, PROJECT_FORMAT, PROJECT_VERSION, Project, resolve_media_path,
    stored_media_path,
};

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(
        "unsupported project version {found}: this build supports version {supported}. The project was written by a newer Splice; refusing to guess its schema."
    )]
    NewerVersion { found: u64, supported: u32 },
    #[error("unsupported project version {found}: no migration from this version is available")]
    UnsupportedVersion { found: u64 },
    #[error("not a Splice project: expected top-level format \"splice-project\", found {found}")]
    WrongFormat { found: String },
    #[error("invalid project: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub project: Project,
    pub path: PathBuf,
}

pub fn load_project(path: &Path) -> Result<LoadedProject> {
    ensure_extension(path)?;
    let absolute = absolute_path(path)?;
    let bytes = fs::read(&absolute)
        .with_context(|| format!("could not read project {}", absolute.display()))?;
    let text = String::from_utf8(bytes)
        .with_context(|| format!("project {} is not valid UTF-8", absolute.display()))?;
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("project {} is not valid JSON", absolute.display()))?;

    let object = value
        .as_object()
        .ok_or_else(|| ProjectError::Invalid("top-level JSON value must be an object".into()))?;

    let format = object
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if format != PROJECT_FORMAT {
        return Err(ProjectError::WrongFormat { found: format }.into());
    }

    let version_value = object
        .get("version")
        .ok_or_else(|| ProjectError::Invalid("missing integer version".into()))?;
    let version = version_value
        .as_u64()
        .ok_or_else(|| ProjectError::Invalid("version must be a non-negative integer".into()))?;

    if version > PROJECT_VERSION as u64 {
        return Err(ProjectError::NewerVersion {
            found: version,
            supported: PROJECT_VERSION,
        }
        .into());
    }
    if version < 1 {
        return Err(ProjectError::UnsupportedVersion { found: version }.into());
    }

    let value = migrate(value, version)?;
    let mut project: Project = serde_json::from_value(value)
        .with_context(|| format!("invalid project schema in {}", absolute.display()))?;

    if project.format != PROJECT_FORMAT {
        return Err(ProjectError::WrongFormat {
            found: project.format,
        }
        .into());
    }
    if project.version != PROJECT_VERSION {
        project.version = PROJECT_VERSION;
    }
    if project.name.trim().is_empty() {
        project.name = absolute
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Untitled")
            .to_string();
    }
    validate_project(&project)?;

    Ok(LoadedProject {
        project,
        path: absolute,
    })
}

pub fn save_project(
    path: &Path,
    project: &mut Project,
    current_project_dir: Option<&Path>,
) -> Result<()> {
    ensure_extension(path)?;
    let destination = absolute_path(path)?;
    let directory = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("project path has no parent directory"))?;
    fs::create_dir_all(directory)
        .with_context(|| format!("could not create {}", directory.display()))?;

    project.format = PROJECT_FORMAT.to_string();
    project.version = PROJECT_VERSION;
    validate_project(project)?;
    normalize_paths(project, current_project_dir, directory);

    let serialized = serde_json::to_string_pretty(project).context("serialize project")?;

    // Same-directory temp file + fsync + atomic rename. A crash can leave the
    // previous project intact, but never a truncated project.
    let mut temp = tempfile::NamedTempFile::new_in(directory)
        .with_context(|| format!("could not create temp file in {}", directory.display()))?;
    temp.write_all(serialized.as_bytes())
        .context("write temporary project")?;
    temp.write_all(b"\n").context("write project newline")?;
    temp.as_file_mut().sync_all().context("fsync project")?;
    temp.persist(&destination)
        .map_err(|error| error.error)
        .with_context(|| format!("atomically replace {}", destination.display()))?;
    if let Ok(dir) = fs::File::open(directory) {
        let _ = dir.sync_all();
    }
    Ok(())
}

pub fn normalize_paths(
    project: &mut Project,
    current_project_dir: Option<&Path>,
    destination_dir: &Path,
) {
    for asset in &mut project.media {
        let stored = Path::new(&asset.path);
        let absolute = if stored.is_absolute() {
            stored.to_path_buf()
        } else if let Some(current) = current_project_dir {
            current.join(stored)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(stored)
        };
        asset.path = stored_media_path(destination_dir, &absolute);
    }
}

fn validate_project(project: &Project) -> Result<()> {
    if project.settings.fps == 0 {
        return Err(ProjectError::Invalid("fps must be greater than 0".into()).into());
    }
    if project.settings.width == 0 || project.settings.height == 0 {
        return Err(ProjectError::Invalid("project resolution must be non-zero".into()).into());
    }
    let mut ids = std::collections::HashSet::new();
    for asset in &project.media {
        if !ids.insert(asset.id.clone()) {
            return Err(ProjectError::Invalid(format!("duplicate media id {}", asset.id)).into());
        }
        if asset.path.trim().is_empty() {
            return Err(
                ProjectError::Invalid(format!("media {} has an empty path", asset.id)).into(),
            );
        }
    }
    for track in &project.tracks {
        for clip in &track.clips {
            if clip.duration() <= 0.0 || !clip.duration().is_finite() {
                return Err(ProjectError::Invalid(format!(
                    "clip {} has a non-positive duration",
                    clip.id
                ))
                .into());
            }
            if !clip.start.is_finite() || clip.start < 0.0 {
                return Err(ProjectError::Invalid(format!(
                    "clip {} has an invalid timeline start",
                    clip.id
                ))
                .into());
            }
            if clip.media_id.is_none() && clip.title.is_none() {
                return Err(ProjectError::Invalid(format!(
                    "clip {} is neither a media clip nor a title",
                    clip.id
                ))
                .into());
            }
        }
    }
    Ok(())
}

fn ensure_extension(path: &Path) -> Result<()> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case(PROJECT_EXTENSION) => Ok(()),
        _ => bail!(
            "project files must use the .{PROJECT_EXTENSION} extension: {}",
            path.display()
        ),
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("current directory is unavailable")?
            .join(path))
    }
}

fn migrate(value: Value, from_version: u64) -> Result<Value> {
    // Version 1 is the locked initial schema. When a schema change lands, add
    // an explicit step here (and document it in docs/PROJECT_FORMAT.md).
    match from_version {
        v if v == PROJECT_VERSION as u64 => Ok(value),
        other => Err(ProjectError::UnsupportedVersion { found: other }.into()),
    }
}

pub fn missing_media_paths(project: &Project, project_dir: &Path) -> Vec<PathBuf> {
    project
        .media
        .iter()
        .filter_map(|asset| {
            let path = resolve_media_path(project_dir, asset);
            if path.is_file() { None } else { Some(path) }
        })
        .collect()
}

/// Used by tests and the demo generator to confirm that the on-disk project is
/// exactly the versioned envelope this build promises.
pub fn project_envelope(project: &Project) -> Value {
    json!({
        "format": project.format,
        "version": project.version,
    })
}
