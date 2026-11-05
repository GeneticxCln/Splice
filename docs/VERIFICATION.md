# Splice Verification Log

This file is the append-only evidence log for the Splice delivery. Every gate is
verified with a real command against the running environment. Nothing in this
file is synthetic: commands, exit statuses and relevant output are pasted from
the machine that built and ran the binary.

Hard prerequisites: a real Wayland display, a real default microphone, system
ffmpeg/ffprobe, bubblewrap, and a working screenshot path.

## Slice 0 — environment probes (recorded before implementation)

Date: 2026-11-05
Host: Linux (CachyOS), user `quinton`, 16 cores.
Working tree: `/home/quinton/Projects/Splice`.

### FFmpeg

Command:

```sh
ffmpeg -version
```

Exit status: 0. Key output:

```text
ffmpeg version n9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
configuration: ... --enable-gpl ... --enable-libass ... --enable-libx264 ...
libavutil      61.  1.102 / 61.  1.102
libavcodec     63.  1.102 / 63.  1.102
libavformat    63.  1.102 / 63.  1.102
```

Required encoders:

```sh
ffmpeg -hide_banner -encoders 2>/dev/null | grep -E '^\s*[VAS].....\s+(libx264|aac)\s'
```

Exit status: 0. Key output:

```text
 V....D libx264              libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 A....D aac                  AAC (Advanced Audio Coding)
```

### FFprobe

Command:

```sh
ffprobe -version
```

Exit status: 0. Key output:

```text
ffprobe version n9.0.2 Copyright (c) 2007-2026 the FFmpeg developers
libavutil      61.  1.102 / 61.  1.102
libavcodec     63.  1.102 / 63.  1.102
```

### Bubblewrap

Command:

```sh
bwrap --version
```

Exit status: 0. Output:

```text
bubblewrap 0.13.1
```

### Display

Commands:

```sh
echo "WAYLAND_DISPLAY=$WAYLAND_DISPLAY"; echo "DISPLAY=$DISPLAY"; echo "XDG_SESSION_TYPE=$XDG_SESSION_TYPE"
ls -la /run/user/$(id -u)/wayland-*
```

Exit status: 0. Key output:

```text
WAYLAND_DISPLAY=wayland-0
DISPLAY=:1
XDG_SESSION_TYPE=wayland
srwxr-xr-x 1 quinton quinton 0  5. Nov 14:01 /run/user/1000/wayland-0
-rw-r----- 1 quinton quinton 0  5. Nov 14:01 /run/user/1000/wayland-0.lock
```

### Default microphone enumeration

Command:

```sh
pactl info | grep -E 'Default Source|Default Sink'
wpctl inspect @DEFAULT_AUDIO_SOURCE@ | head -25
```

Exit status: 0. Key output:

```text
Default Source: alsa_input.usb-Creative_Technology_Ltd_SB_Omni_Surround_5.1_0000001b-00.analog-stereo-input
id 59, type PipeWire:Interface:Node
    alsa.card_name = "SB Omni Surround 5.1"
    alsa.name = "USB Audio"
    audio.channels = "2"
    audio.position = "[ FL, FR ]"
```

### Preliminary microphone level probe (not the Rust proof)

Command:

```sh
ffmpeg -hide_banner -loglevel error -f pulse -i default -t 5 -ac 2 -ar 48000 /tmp/opencode/mic-probe.wav
ffmpeg -hide_banner -i /tmp/opencode/mic-probe.wav -af volumedetect -f null -
```

Exit status: 0 (both). Key output:

```text
Duration: 00:00:04.99 ... 48000 Hz, stereo, s16
[Parsed_volumedetect_0 @ ...] mean_volume: -21.8 dB
[Parsed_volumedetect_0 @ ...] max_volume: -8.7 dB
```

The default source delivered real non-silent audio. The Rust/cpal proof is
recorded below.

### Screenshot path

Command:

```sh
grim /tmp/opencode/probe-grim.png
```

Exit status: 1. Output:

```text
compositor doesn't support the screen capture protocol
```

`grim` is therefore not a usable path in this KWin/Wayland session. The
xdg-desktop-portal Screenshot interface is present (introspection exit 0):

```text
interface org.freedesktop.portal.Screenshot {
  Screenshot(in  s parent_window, in  a{sv} options, out o handle);
```

The chosen path for `docs/screenshot.png` is eframe's real framebuffer capture
(`ViewportCommand::Screenshot`). It is confirmed below with the actual Splice
window; the evidence is a screenshot of the running application.

### Eframe screenshot probe

A real eframe window was compiled and run with the title `Splice`:

```sh
cargo build --release
./target/release/splice /tmp/opencode/eframe-probe.png
```

Exit status: 0. Output:

```text
SCREENSHOT_OK /tmp/opencode/eframe-probe.png 1920x2067
```

The capture is a 1920x2067 RGBA PNG of the actual rendered framebuffer
(`ViewportCommand::Screenshot` -> `Event::Screenshot`). **Chosen screenshot
path: eframe ViewportCommand::Screenshot.** `grim` is unavailable in this
compositor; no portal interaction is needed.

Bubblewrap, the Wayland socket, system ffmpeg/ffprobe and the default source are
all present. The hard prerequisites are met, so implementation continues.

## Slice 1 — build, formatting and static analysis

Date: 2026-11-05. Working tree: `/home/quinton/Projects/Splice`.

```sh
cargo fmt --check
```

Exit status: 0.

```sh
cargo clippy --all-targets -- -D warnings
```

Exit status: 0. The audit found 14 pre-existing lints (9 lib, 4 bin, 1 test:
collapsible `if`, needless borrow, `&PathBuf` arguments, a derivable `Default`,
an assign-op pattern and `trim` before `split_whitespace`). They were repaired
mechanically; no lint was suppressed and no behaviour changed.

```sh
cargo build --release
```

Exit status: 0 (`Finished release profile` after 4.90 s).

```sh
cargo test
```

Exit status: 0. Results:

```text
unittests src/lib.rs          6 passed; 0 failed
unittests src/main.rs         0 passed; 0 failed
tests/full_suite.rs           8 passed; 0 failed
tests/render_smoke.rs         2 passed; 0 failed
Doc-tests                     0 passed; 0 failed
```

The 8 acceptance tests that ran are `catalog_and_presets_are_complete`,
`project_roundtrip_is_versioned_and_rejects_newer_files`,
`offline_assets_are_listed_and_relink_is_validated`,
`cli_export_succeeds_and_blocks_missing_media_without_partial_output`,
`title_pan_zoom_monochrome_and_audio_fades_are_measured`,
`automovie_builds_titles_transitions_and_music`,
`feature_matrix_renders_all_transitions_effects_and_titles`, and
`full_feature_export_matches_every_preset`.

Gate status: PASS.

## Slice 2 — export presets and offline blocking

Commands:

```sh
for p in 1080p 720p 480p; do
  ./target/release/splice export --project docs/demo.splice \
      --preset "$p" --output "/tmp/final-export/demo-$p.mp4"
done
```

All three exited 0 (`EXPORT_OK ... duration=21.50s`). `ffprobe` on each output:

| Preset | Codec | Profile / level | Size | FPS | Audio | Duration |
| --- | --- | --- | --- | --- | --- | --- |
| `1080p` | h264 / aac | High / 41 | 1920 × 1080 | 30/1 | AAC LC, 48 000 Hz, 2 ch | 21.500000 s |
| `720p` | h264 / aac | High / 32 | 1280 × 720 | 30/1 | AAC LC, 48 000 Hz, 2 ch | 21.500000 s |
| `480p` | h264 / aac | High / 31 | 854 × 480 | 30/1 | AAC LC, 48 000 Hz, 2 ch | 21.500000 s |

Offline refusal was exercised on a copy with `clip-a.mp4` moved away:

```text
$ ./target/release/splice export --project /tmp/offline/demo.splice \
      --preset 480p --output /tmp/offline/out.mp4
splice: export blocked: 1 missing media asset(s) in /tmp/offline/demo.splice
  clip-a => /tmp/offline/media/clip-a.mp4
```

Exit status: 1. `/tmp/offline/out.mp4` was not created and no
`.splice-export-*` temporary file was left behind.

Gate status: PASS.

## Slice 3 — reproducible demo and real GUI screenshot

```sh
bash scripts/demo.sh
```

Exit status: 0. Key output:

```text
DEMO_PROJECT_OK docs/demo.splice duration=21.50s media=5 video_clips=5 title_clips=2
EXPORT_OK docs/demo.mp4 preset=1080p duration=21.50s
codec_name=h264
width=1920
height=1080
r_frame_rate=30/1
codec_name=aac
sample_rate=48000
channels=2
SCREENSHOT_OK docs/screenshot.png 1913x2058
DEMO_OK project=docs/demo.splice video=docs/demo.mp4 screenshot=docs/screenshot.png
```

`docs/demo.mp4` probes at 21.500000 s. `docs/screenshot.png` is a real eframe
framebuffer capture (`ViewportCommand::Screenshot`) of the running Splice
window with `docs/demo.splice` open at playhead 2.5 s; it shows the media
library, proxy preview with the title overlay, inspector and timeline.

Gate status: PASS.

## Slice 4 — default microphone capture through cpal

```sh
./target/release/splice voicecheck --seconds 2 --output /tmp/final-mic.wav
```

Exit status: 0. Output:

```text
DEFAULT_INPUT default (ALSA) 48000 Hz, 2 ch, F32
VOICECHECK device="default" host=ALSA sample_rate=48000 channels=2 seconds=1.963 rms_dbfs=-40.22 peak_dbfs=-27.28 non_silent=true path=/tmp/final-mic.wav
```

The capture contains real, non-silent audio (RMS above the −50 dBFS and peak
above the −40 dBFS thresholds), so the voiceover path is proven against the
machine's actual default input.

Gate status: PASS.

## Slice 5 — audit result and final gate

Final gate status: **PASS**. The binary builds cleanly in debug and release,
formatting and `clippy -D warnings` are clean, all 16 deterministic tests pass, all three
presets encode with the documented streams, offline media blocks export
without partial output, the demo is reproducible end-to-end, the GUI screenshot
is a real framebuffer capture, and cpal captures the real default microphone.

The audit's preview-monitoring gap (silent playback) is closed in Slice 6;
preview playback now feeds the proxy soundtrack to the default output.

## Slice 6 — preview audio monitoring (audit limitation closed)

Implementation: `AudioMonitor` in `src/audio.rs` decodes the preview proxy with
ffmpeg to `f32le` at the output device's rate and channel count, feeds a bounded
two-second queue from a decoder thread, and plays it through cpal. `Preview`
starts and stops the monitor with playback, restarts it on seek, and uses the
detected ffmpeg path for both video and audio decoding instead of a hard-coded
binary name.

Hardware gate (the test is `#[ignore]` so headless machines can still run the
deterministic suite):

```sh
cargo test --lib audio::tests::audio_monitor_plays_decoded_audio -- --ignored --nocapture
```

Exit status: 0. Output:

```text
running 1 test
test audio::tests::audio_monitor_plays_decoded_audio ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.65s
```

The test asserts that the real default output callback consumed decoded frames
(`played_frames > 0`). The default sink at the time was
`alsa_output.usb-Creative_Technology_Ltd_SB_Omni_Surround_5.1_...analog-stereo-output`.

The same hardware gate is repeated through the actual Play path
(`Preview::toggle_playback`, not just the monitor in isolation):

```sh
cargo test --lib preview::tests::playback_starts_audio_monitor -- --ignored --nocapture
```

Exit status: 0. Output:

```text
running 1 test
test preview::tests::playback_starts_audio_monitor ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.69s
```

Deterministic gates after the change:

```text
cargo fmt --check                                    exit 0
cargo clippy --all-targets -- -D warnings           exit 0
cargo test      6 passed + 2 ignored (lib), 8 passed (full_suite), 2 passed (render_smoke); 0 failed
```

Gate status: PASS.

## Slice 7 — repository and README

The audit found no git repository and no README. Both are now in place:
`git init` created the history, `.gitignore` keeps `target/` and the generated
demo media/video/project out of it, and `README.md` documents the build, the GUI
and CLI, the reproducible demo, the project and export contracts, the hardware
audio gate, and the remaining product limitations.

The initial commit is `38dbfa0` ("Initial commit: Splice native Linux timeline
editor"). The two hardware audio gates are `#[ignore]` by design so `cargo test`
stays deterministic on headless machines; both were run explicitly above.

## Slice 8 — full transition and effect catalogs

The artificial 10-transition / 10-effect cap is gone. `TransitionKind` now
exposes all 58 `xfade` transitions reported by the installed ffmpeg n9.0.2
(IDs 0–57, excluding `custom`), and `EffectKind` exposes 38 effects backed by
real ffmpeg filters. Enum values for the original catalog entries are unchanged
and the project schema remains version 1, so existing `.splice` files still
load.

Catalog completeness gate:

```text
$ cargo test --test full_suite catalog_and_presets_are_complete
running 1 test
test catalog_and_presets_are_complete ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured
```

The test checks `TRANSITIONS.len() == TransitionKind::ALL.len()`,
`EFFECTS.len() == EffectKind::ALL.len()`, that every enum variant appears in the
catalog, that xfade names and effect kinds are unique, and that every effect
parameter set materializes without duplicate keys. Title styles remain at 10.

Rendering gate — the rewritten matrix test builds one project containing all 58
transitions and another containing all 38 effects, renders both at 480×270 in
preview mode through the same render graph used by the GUI, and probes h264 at
480×270:

```text
$ cargo test --test full_suite feature_matrix_renders_all_transitions_effects_and_titles
test feature_matrix_renders_all_transitions_effects_and_titles ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; finished in 11.94s
```

Full gate after the change:

```text
cargo fmt --check                                    exit 0
cargo clippy --all-targets -- -D warnings           exit 0
cargo test      6 passed + 2 ignored (lib), 8 passed (full_suite), 2 passed (render_smoke); 0 failed
```

The two ignored tests are the existing hardware audio gates from Slice 6.
Gate status: PASS.

## Slice 9 — categorized/searchable pickers and refreshed demo

The effect picker is grouped into five categories (`Tone & Colour`,
`Blur & Detail`, `Stylize`, `Distortion`, `Motion`) and both the effect and
transition menus have live search fields. `EFFECT_CATEGORIES` in
`src/catalog.rs` is the single source of truth; the catalog test asserts every
`EffectKind` appears in exactly one category.

Demo refresh — `examples/make_demo.rs` now exercises `ColorBalance`,
`Vibrance`, `FilmGrain`, `Deband`, `FilmCurve` and `ChromaShift`, with the
`SmoothLeft`, `CircleCrop`, `Radial` and `HBlur` transitions. The saved project
inventory is:

```text
clip-a          transition=None         effects=brightness,contrast,saturation,color_balance,vibrance
clip-a (split)  transition=smooth_left  effects=brightness,contrast,saturation,color_balance,vibrance,temperature,film_grain
clip-b          transition=circle_crop  effects=vignette,sharpen,deband
photo           transition=radial       effects=pan_zoom,blur,film_curve
clip-c          transition=h_blur       effects=hue_rotate,chroma_shift
```

`bash scripts/demo.sh` exited 0 and regenerated the artifacts:

```text
DEMO_PROJECT_OK docs/demo.splice duration=21.50s media=5 video_clips=5 title_clips=2
EXPORT_OK docs/demo.mp4 preset=1080p duration=21.50s
codec_name=h264 / 1920x1080 / 30 fps
codec_name=aac / 48000 Hz / 2 ch
SCREENSHOT_OK docs/screenshot.png 1920x2067
DEMO_OK project=docs/demo.splice video=docs/demo.mp4 screenshot=docs/screenshot.png
```

The screenshot is a real eframe framebuffer capture at playhead 2.5 s; the
inspector shows the new `Color Balance` and `Vibrance` effects alongside the
three original tone effects, and the timeline contains the four new transition
kinds.

Gates after the change:

```text
cargo fmt --check                                    exit 0
cargo clippy --all-targets -- -D warnings           exit 0
cargo test      6 passed + 2 ignored (lib), 8 passed (full_suite), 2 passed (render_smoke); 0 failed
```

Gate status: PASS.

## Slice 10 — capability probes, font fallback and audio recovery

`Toolchain::detect` now probes the ffmpeg `ass` filter and `fc-match`
availability. The status bar and About window list missing libx264, AAC, ass or
fontconfig capabilities, and the toolbar badge changes from `FFMPEG READY` to
`FFMPEG LIMITED`.

The title editor validates the selected font with `fc-match` (cached per family)
and warns when libass will substitute a different family. On this host:

```text
$ fc-match -f '%{family}\n' 'DejaVu Sans'
DejaVu Sans
$ fc-match -f '%{family}\n' 'Definitely Missing Splice Font 12345'
Noto Sans
```

`AudioMonitor::supervise` now rebuilds the default cpal output after a stream
error, rate-limited to one rebuild per 750 ms and resuming from the live preview
playhead. `Preview::tick` invokes it during playback and clears the audio error
after recovery.

The crate is on edition 2024. `scripts/check-msrv.sh` checks Rust 1.85.0 when
rustup is available, and `.github/workflows/msrv.yml` runs the same check in CI.
The local host has no rustup or 1.85 toolchain, so the MSRV script was not run
here; the current Rust 1.99.1 toolchain compiled the edition-2024 crate.

Evidence:

```text
cargo fmt --check                                    exit 0
cargo clippy --all-targets -- -D warnings           exit 0
cargo check --all-targets                            exit 0
cargo test      7 passed + 3 ignored (lib), 8 passed (full_suite), 2 passed (render_smoke); 0 failed
missing_font_reports_a_fallback_warning             1 passed
audio_monitor_rebuilds_after_output_error           1 passed (0.86s)
audio_monitor_plays_decoded_audio                   1 passed (0.65s)
playback_starts_audio_monitor                       1 passed (0.69s)
```

Gate status: PASS, with the MSRV CI job unexecuted locally.

## Slice 11 — MSRV verified on Rust 1.88

rustup was installed after Slice 10, and Rust 1.85.0 was installed for the
original gate. The 1.85.0 check failed before compiling: the locked graph
requires newer compilers for `image 0.25.10`, `wayland-protocols 0.32.13`,
`idna_adapter 1.2.2`, `calloop 0.14.5` and ICU 2.4 (Rust 1.86–1.88). The
declared MSRV was raised to 1.88 rather than pinning older transitive crates.

The toolchain check in `scripts/check-msrv.sh` now uses `rustup run`, which
matches rustup's `1.88.0-x86_64-unknown-linux-gnu` label. The script default,
`Cargo.toml`, README and CI workflow all declare Rust 1.88.

Evidence with rustc 1.88.0 (6b00bc388) and cargo 1.88.0:

```text
bash scripts/check-msrv.sh
== checking with Rust 1.88.0 (MSRV) ==
cargo +1.88.0 check --all-targets --locked          exit 0 (13.55s)
```

Gate status: PASS.

## Slice 12 — Shorts title safety and screenshot

Date: 2026-11-05. Version: 1.2.0. Working tree:
`/home/quinton/Projects/Splice`.

Portrait titles now wrap to a 70% wide safe column (15% side margins) and are
clamped between the 10% top and 80% bottom bounds, leaving the lower 20% for
platform controls. Lower thirds start at the safe left edge; credit rolls keep
the safe width while continuing to scroll. Landscape layout is unchanged, and
the unit test `landscape_titles_keep_the_legacy_layout` asserts that no wrapping
or safe-area clamping is applied there.

Focused title tests:

```text
$ cargo test --lib titles::tests -- --nocapture
running 8 tests
test titles::tests::portrait_titles_wrap_inside_the_safe_column ... ok
test titles::tests::portrait_lower_third_starts_at_the_safe_left_edge ... ok
test titles::tests::landscape_titles_keep_the_legacy_layout ... ok
...
test result: ok. 8 passed; 0 failed; 0 ignored
```

Full gate:

```text
cargo fmt --check                                      exit 0
cargo clippy --all-targets --locked -- -D warnings     exit 0
cargo test --locked                                    exit 0
cargo build --release --locked                         exit 0

unittests src/lib.rs          12 passed; 3 ignored (hardware audio)
tests/full_suite.rs            9 passed; 0 failed
tests/render_smoke.rs          3 passed; 0 failed
```

The Shorts screenshot was captured from a temporary 9:16 derivative of
`docs/demo.splice` (media paths made absolute, `framing=fill`):

```text
SHORT_PROJECT_OK settings= {'width': 1080, 'height': 1920, 'fps': 30, 'sample_rate': 48000, 'background': '#0d1016', 'framing': 'fill'}
SCREENSHOT_OK /home/quinton/Projects/Splice/docs/shorts.png 1913x2058
```

`docs/shorts.png` is a real eframe framebuffer capture of the running Splice
window at playhead 2.5 s. The preview is `540x960`, the safe-area border and
side shades are visible, and the demo title/subtitle wrap inside the safe
column. The large `SPLICE DEM` text at the bottom is baked into the source
footage and is intentionally center-cropped by Fill; it is not the editable
title overlay.

The encoded path was checked independently:

```text
$ ./target/release/splice export --project /tmp/splice-shorts-demo.splice \
    --preset shorts-1080p --output /tmp/splice-short-title-check.mp4
EXPORT_OK /tmp/splice-short-title-check.mp4 preset=shorts-1080p duration=21.50s
codec_name=h264
width=1080
height=1920
level=41
r_frame_rate=30/1
```

The extracted frame at 2.5 s shows `Splice Demo Reel` as two wrapped lines and
the subtitle inside the safe column, so the preview and export paths agree.

Gate status: PASS.
