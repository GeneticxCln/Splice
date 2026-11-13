//! Native file choosers through `zenity` or `kdialog`, when one is installed.
//!
//! Splice has no GUI toolkit dependency for this: the desktop's own chooser is
//! launched as a child process, so it matches the user's theme and portals.
//! Without either tool the typed-path fields keep working on their own.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Zenity,
    Kdialog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Open,
    OpenMany,
    Save,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub mode: Mode,
    pub title: String,
    /// Folder or file the chooser starts at.
    pub start: PathBuf,
    /// (label, glob patterns without the leading `*.`).
    pub filters: Vec<(String, Vec<String>)>,
}

/// The first chooser found on `PATH`; KDE's is preferred inside a KDE session.
pub fn detect() -> Option<Backend> {
    let kde = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|desktop| desktop.to_ascii_uppercase().contains("KDE"))
        .unwrap_or(false);
    let order = if kde {
        [Backend::Kdialog, Backend::Zenity]
    } else {
        [Backend::Zenity, Backend::Kdialog]
    };
    order.into_iter().find(|backend| on_path(program(*backend)))
}

fn program(backend: Backend) -> &'static str {
    match backend {
        Backend::Zenity => "zenity",
        Backend::Kdialog => "kdialog",
    }
}

fn on_path(binary: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|directory| directory.join(binary).is_file())
}

/// Builds the chooser command line. Kept free of side effects so the exact
/// arguments can be tested without a display.
pub fn command(backend: Backend, request: &Request) -> Command {
    let mut command = Command::new(program(backend));
    match backend {
        Backend::Zenity => {
            command.args(["--file-selection", "--title", &request.title]);
            match request.mode {
                Mode::Open => {}
                Mode::OpenMany => {
                    command.args(["--multiple", "--separator", "\n"]);
                }
                Mode::Save => {
                    command.args(["--save", "--confirm-overwrite"]);
                }
            }
            command.arg(format!("--filename={}", start_argument(&request.start)));
            for (label, patterns) in &request.filters {
                let globs: Vec<String> = patterns.iter().map(|ext| format!("*.{ext}")).collect();
                command.arg(format!("--file-filter={label} | {}", globs.join(" ")));
            }
        }
        Backend::Kdialog => {
            command.args(["--title", &request.title]);
            let start = start_argument(&request.start);
            let filter = request
                .filters
                .first()
                .map(|(label, patterns)| {
                    let globs: Vec<String> =
                        patterns.iter().map(|ext| format!("*.{ext}")).collect();
                    format!("{}|{label}", globs.join(" "))
                })
                .unwrap_or_default();
            match request.mode {
                Mode::Open => {
                    command.args(["--getopenfilename", &start, &filter]);
                }
                Mode::OpenMany => {
                    command.args(["--multiple", "--separate-output"]);
                    command.args(["--getopenfilename", &start, &filter]);
                }
                Mode::Save => {
                    command.args(["--getsavefilename", &start, &filter]);
                }
            }
        }
    }
    command
}

/// Zenity wants a trailing `/` to open inside a folder rather than select it.
fn start_argument(start: &Path) -> String {
    let text = start.display().to_string();
    if start.is_dir() && !text.ends_with('/') {
        format!("{text}/")
    } else {
        text
    }
}

/// One chosen path per non-empty line.
pub fn parse_output(stdout: &str) -> Vec<PathBuf> {
    stdout
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Shows the chooser and blocks until it closes. An empty list means the user
/// cancelled (both tools exit with status 1 for that).
pub fn pick(backend: Backend, request: &Request) -> Result<Vec<PathBuf>> {
    let output = command(backend, request)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("could not start {}", program(backend)))?;
    if output.status.success() {
        return Ok(parse_output(&String::from_utf8_lossy(&output.stdout)));
    }
    if output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    anyhow::bail!(
        "{} failed ({}): {}",
        program(backend),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn request(mode: Mode) -> Request {
        Request {
            mode,
            title: "Open project".into(),
            start: PathBuf::from("/nonexistent/movie.splice"),
            filters: vec![("Splice projects".into(), vec!["splice".into()])],
        }
    }

    #[test]
    fn zenity_open_save_and_multiple_arguments() {
        let open = args(&command(Backend::Zenity, &request(Mode::Open)));
        assert_eq!(open[0], "--file-selection");
        assert!(open.contains(&"--filename=/nonexistent/movie.splice".to_string()));
        assert!(open.contains(&"--file-filter=Splice projects | *.splice".to_string()));
        assert!(!open.contains(&"--save".to_string()));

        let save = args(&command(Backend::Zenity, &request(Mode::Save)));
        assert!(save.contains(&"--save".to_string()));
        assert!(save.contains(&"--confirm-overwrite".to_string()));

        let many = args(&command(Backend::Zenity, &request(Mode::OpenMany)));
        assert!(many.contains(&"--multiple".to_string()));
        assert!(many.windows(2).any(|pair| pair == ["--separator", "\n"]));
    }

    #[test]
    fn kdialog_arguments() {
        let open = args(&command(Backend::Kdialog, &request(Mode::Open)));
        assert_eq!(
            open,
            [
                "--title",
                "Open project",
                "--getopenfilename",
                "/nonexistent/movie.splice",
                "*.splice|Splice projects"
            ]
        );
        let many = args(&command(Backend::Kdialog, &request(Mode::OpenMany)));
        assert!(many.contains(&"--multiple".to_string()));
        assert!(many.contains(&"--separate-output".to_string()));
        let save = args(&command(Backend::Kdialog, &request(Mode::Save)));
        assert!(save.contains(&"--getsavefilename".to_string()));
    }

    #[test]
    fn folders_get_a_trailing_slash_so_the_chooser_opens_inside_them() {
        let dir = tempfile::tempdir().unwrap();
        assert!(start_argument(dir.path()).ends_with('/'));
        assert_eq!(
            start_argument(Path::new("/no/such/file.mp4")),
            "/no/such/file.mp4"
        );
    }

    #[test]
    fn output_is_one_path_per_line_and_ignores_blank_lines() {
        let paths = parse_output("/a/one.mp4\n/b/two words.mp4\n\n");
        assert_eq!(
            paths,
            [
                PathBuf::from("/a/one.mp4"),
                PathBuf::from("/b/two words.mp4")
            ]
        );
        assert!(parse_output("").is_empty());
    }
}
