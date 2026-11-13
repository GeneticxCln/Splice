# Changelog

## 1.4.0

### Fixed
- Preview playback is paced to the playhead clock. Video previously decoded at
  full speed and ran ahead of the audio.
- Clips with about 90 or more motion keyframes failed to render because ffmpeg
  rejects expressions nested around 100 levels deep. Keyframe curves are now a
  balanced expression tree, so the documented 512-keyframe limit works.
- Keyframes that land on the same output frame no longer drop later keyframes.
- Large projects no longer fail at launch: the filter graph is passed to ffmpeg
  by file instead of as one command-line argument (128 KiB kernel limit).
- Failed split, duplicate, delete and add operations no longer create undo
  entries or mark the project as edited.
- AutoMovie replaces only the titles it generated; titles you wrote with the
  same text are kept.
- Voiceover recordings are written atomically.
- Titles on the primary video track (reachable only by hand-editing a project)
  are rejected on save instead of failing at export.
- Font names can no longer inject extra lines into the generated ASS file.

### Added
- Export progress bar and a Cancel button. Cancelling, or quitting during an
  export, stops ffmpeg and removes the temporary file.
- Unsaved-changes prompt for New, Open, dropping a project, Quit and closing
  the window.
- Autosave next to the project every 30 seconds while it has unsaved edits, and
  an offer to recover it after an unclean exit.
- Media probing runs off the UI thread.
- A change to the project cancels a stale in-flight preview render.
- Each clip's source is opened at its in-point (`-ss`/`-t`) instead of being
  decoded from the start of the file.
- Media is opened with a file-only protocol whitelist.
- `packaging/`: desktop entry, icon, MIME type and `install.sh`.
- Unit tests for the edit operations, autosave, the runner, preview pacing and
  motion expressions; integration tests for seek accuracy, oversized graphs and
  export cancellation.

### Changed
- `render_project` is split: effect filters live in `filters.rs`, the audio mix
  is its own function, and ffmpeg is driven through `runner.rs`.
- The UI shows the 30 fps / 48 kHz the renderer actually uses.
- ffmpeg 7 and later are supported for graph files (`-/filter_complex`);
  older versions use `-filter_complex_script`.

## 1.3.1 and earlier

See the GitHub releases page.
