# Splice export presets

Presets are locked by the product contract. There is no custom width/height,
frame-rate, codec, or bitrate UI: the same graph is used by the GUI and the CLI,
so a project exports identically from either path.

## Preset table

| Preset | Resolution | H.264 level |
| --- | --- | --- |
| `1080p` | 1920 × 1080 | 4.1 |
| `720p` | 1280 × 720 | 3.2 |
| `480p` | 854 × 480 | 3.1 |

## Common encoding settings

| Setting | Value |
| --- | --- |
| Container | MP4 (`+faststart`, moov atom moved to the front) |
| Video codec | H.264 / AVC (`libx264`) |
| Video profile | high |
| Rate control | CRF 20, preset `medium` |
| Pixel format | `yuv420p` |
| Frame rate | 30 fps |
| GOP | 60 frames, minimum 30, scene-cut detection off |
| Audio codec | AAC |
| Audio bitrate | 192 kbit/s |
| Sample rate | 48 000 Hz |
| Channels | stereo (2) |
| Scaling | aspect-preserving, padded to the preset canvas with `#101318` |

The preview proxy uses the same graph with `ultrafast`, CRF 28, level 3.1, AAC
128 kbit/s at 960 × 540. It is never written to the final output path.

## CLI

```sh
splice export --project <file.splice> --preset <1080p|720p|480p> --output <file.mp4>
```

Exit status is `0` only after the complete file has been encoded, checked for a
non-zero size, and atomically renamed into place. Any failure exits non-zero and
removes the temporary partial file. Export is refused, with no output file, when:

- the timeline is empty;
- any media asset is missing (offline);
- the output path does not end in `.mp4`;
- the output path resolves to one of the project's source media files.

## GUI

`Export Movie` opens the same presets and calls the same `export_project`
function on a background thread, reporting `Exported …` or the exact error in
the dialog and status bar. The Export button is enabled only while ffmpeg is
detected.
