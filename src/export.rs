use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::ffmpeg::Toolchain;
use crate::model::resolve_media_path;
use crate::presets::ExportPreset;
use crate::render::{RenderOptions, render_project};
use crate::runner::RenderControl;

#[derive(Debug, Clone)]
pub struct ExportReport {
    pub output: PathBuf,
    pub preset: ExportPreset,
    pub duration: f64,
    pub command: String,
    pub graph_bytes: usize,
}

/// GUI and CLI share this function so both produce identical encodes and both
/// enforce the same missing-media and source-protection rules.
pub fn export_project(
    project: &crate::model::Project,
    project_dir: &Path,
    tools: &Toolchain,
    preset: ExportPreset,
    output: &Path,
) -> Result<ExportReport> {
    export_project_with(
        project,
        project_dir,
        tools,
        preset,
        output,
        &RenderControl::new(),
    )
}

/// Like `export_project`, but `control` can cancel the encode (the temporary
/// file is removed and the error downcasts to `runner::Cancelled`) and reports
/// encoded seconds while it runs.
pub fn export_project_with(
    project: &crate::model::Project,
    project_dir: &Path,
    tools: &Toolchain,
    preset: ExportPreset,
    output: &Path,
    control: &RenderControl,
) -> Result<ExportReport> {
    tools.require_encoders()?;

    if project.timeline_duration() <= 0.01 {
        bail!("the timeline is empty; there is nothing to export");
    }

    let missing = project.missing_assets(project_dir);
    if !missing.is_empty() {
        let list = missing
            .iter()
            .map(|asset| {
                format!(
                    "  - {}: {}",
                    asset.name,
                    resolve_media_path(project_dir, asset).display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "export blocked: project has missing media. Relink every offline asset and retry.\n{list}"
        );
    }

    if !output
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false)
    {
        bail!("export output must be an .mp4 file: {}", output.display());
    }

    let output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()?.join(output)
    };
    guard_source_overwrite(project, project_dir, &output)?;

    let directory = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("output path has no parent directory"))?;
    std::fs::create_dir_all(directory)
        .with_context(|| format!("could not create output directory {}", directory.display()))?;

    let temp = directory.join(format!(
        ".splice-export-{}-{}.mp4",
        std::process::id(),
        crate::model::new_id("tmp")
    ));

    let options = RenderOptions {
        width: preset.width(),
        height: preset.height(),
        preview: false,
        allow_offline: false,
        level: preset.level().to_string(),
        output: temp.clone(),
        control: control.clone(),
    };

    let result = render_project(project, project_dir, tools, &options);
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
    };

    let size = std::fs::metadata(&temp)
        .with_context(|| format!("export produced no readable file at {}", temp.display()))?
        .len();
    if size == 0 {
        let _ = std::fs::remove_file(&temp);
        bail!("export produced an empty file");
    }

    std::fs::rename(&temp, &output).with_context(|| {
        let _ = std::fs::remove_file(&temp);
        format!(
            "could not move completed export into place at {}",
            output.display()
        )
    })?;

    Ok(ExportReport {
        output,
        preset,
        duration: report.duration,
        command: report.command,
        graph_bytes: report.graph_bytes,
    })
}

fn guard_source_overwrite(
    project: &crate::model::Project,
    project_dir: &Path,
    output: &Path,
) -> Result<()> {
    let output_normalized = normalize_existing(output);
    for asset in &project.media {
        let source = resolve_media_path(project_dir, asset);
        let source_normalized = normalize_existing(&source);
        if source_normalized == output_normalized {
            bail!(
                "refusing to export over source media {}; source files are never rewritten",
                source.display()
            );
        }
    }
    Ok(())
}

fn normalize_existing(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
