use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;

use crate::model::Project;
use crate::project_io::save_project;

/// How often a dirty, already-saved project is written to its autosave file.
pub const AUTOSAVE_INTERVAL_SECS: u64 = 30;

/// Hidden sibling of the project (`.movie.autosave.splice`). It lives in the
/// project's folder so relative media paths stay valid, and it keeps the
/// `.splice` extension so the normal loader can read it.
pub fn autosave_path(project_path: &Path) -> PathBuf {
    let stem = project_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("project");
    project_path.with_file_name(format!(".{stem}.autosave.splice"))
}

/// Writes the working copy next to the project through the same atomic save
/// the project itself uses. The real project file is never touched.
pub fn write_autosave(project_path: &Path, project: &Project, project_dir: &Path) -> Result<()> {
    let mut copy = project.clone();
    save_project(&autosave_path(project_path), &mut copy, Some(project_dir))
}

/// The autosave file, if one exists that is newer than the project file: the
/// signature of a session that ended without saving its last edits.
pub fn pending_recovery(project_path: &Path) -> Option<PathBuf> {
    let autosave = autosave_path(project_path);
    let autosave_time = modified(&autosave)?;
    match modified(project_path) {
        Some(project_time) if project_time >= autosave_time => None,
        _ => Some(autosave),
    }
}

pub fn clear(project_path: &Path) {
    let _ = std::fs::remove_file(autosave_path(project_path));
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_io::load_project;
    use std::time::Duration;

    #[test]
    fn autosave_round_trips_and_only_newer_files_trigger_recovery() {
        let dir = tempfile::tempdir().expect("dir");
        let project_path = dir.path().join("movie.splice");
        let mut saved = Project::new("Saved");
        save_project(&project_path, &mut saved, Some(dir.path())).expect("save");
        assert!(pending_recovery(&project_path).is_none());

        std::thread::sleep(Duration::from_millis(20));
        let mut edited = saved.clone();
        edited.name = "Edited after save".into();
        write_autosave(&project_path, &edited, dir.path()).expect("autosave");
        assert_eq!(
            autosave_path(&project_path).file_name().unwrap(),
            ".movie.autosave.splice"
        );
        let recovery = pending_recovery(&project_path).expect("newer autosave");
        assert_eq!(
            load_project(&recovery).expect("load").project.name,
            "Edited after save"
        );

        // Saving again makes the project newer than the autosave.
        std::thread::sleep(Duration::from_millis(20));
        save_project(&project_path, &mut edited, Some(dir.path())).expect("resave");
        assert!(pending_recovery(&project_path).is_none());

        clear(&project_path);
        assert!(!autosave_path(&project_path).exists());
    }
}
