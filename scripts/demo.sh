#!/usr/bin/env bash
# Reproducible Splice demo:
#   1. generate every fixture locally with ffmpeg lavfi sources (no network),
#   2. probe + import them through the app's real library path,
#   3. build the feature timeline and save docs/demo.splice,
#   4. export docs/demo.mp4 at 1080p through the CLI,
#   5. capture the real GUI to docs/screenshot.png.
#
# Optional environment overrides:
#   DEMO_MEDIA_DIR, DEMO_PROJECT, DEMO_OUTPUT, DEMO_SCREENSHOT
#   SKIP_SCREENSHOT=1  (headless machines; screenshot evidence is then absent)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MEDIA_DIR="${DEMO_MEDIA_DIR:-$ROOT/docs/media}"
PROJECT="${DEMO_PROJECT:-$ROOT/docs/demo.splice}"
OUTPUT="${DEMO_OUTPUT:-$ROOT/docs/demo.mp4}"
SCREENSHOT="${DEMO_SCREENSHOT:-$ROOT/docs/screenshot.png}"

for tool in ffmpeg ffprobe cargo; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "demo: required tool not found: $tool" >&2
        exit 1
    }
done

mkdir -p "$MEDIA_DIR" "$(dirname "$PROJECT")" "$(dirname "$OUTPUT")" "$(dirname "$SCREENSHOT")"

echo "== generating local fixtures in $MEDIA_DIR =="
ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=1280x720:rate=30:duration=6" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=6" \
    -vf "drawtext=fontfile=/usr/share/fonts/TTF/DejaVuSans-Bold.ttf:text='SPLICE DEMO':fontcolor=white:fontsize=72:x=(w-text_w)/2:y=h-150:box=1:boxcolor=black@0.45:boxborderw=24" \
    -af "volume=0.35,aformat=channel_layouts=stereo" \
    -c:v libx264 -preset veryfast -crf 22 -pix_fmt yuv420p \
    -c:a aac -b:a 192k -shortest "$MEDIA_DIR/clip-a.mp4"

ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "mandelbrot=size=1280x720:rate=30" \
    -f lavfi -i "sine=frequency=330:sample_rate=48000:duration=6" \
    -af "volume=0.3,aformat=channel_layouts=stereo" \
    -t 6 -c:v libx264 -preset veryfast -crf 22 -pix_fmt yuv420p \
    -c:a aac -b:a 192k -shortest "$MEDIA_DIR/clip-b.mp4"

ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "smptebars=size=1280x720:rate=30:duration=6" \
    -f lavfi -i "sine=frequency=550:sample_rate=48000:duration=6" \
    -af "volume=0.25,aformat=channel_layouts=stereo" \
    -c:v libx264 -preset veryfast -crf 22 -pix_fmt yuv420p \
    -c:a aac -b:a 192k -shortest "$MEDIA_DIR/clip-c.mp4"

ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "testsrc2=size=1920x1080:rate=1:duration=1" \
    -frames:v 1 "$MEDIA_DIR/photo.png"

ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "sine=frequency=220:sample_rate=48000:duration=24" \
    -af "volume=0.3,aformat=channel_layouts=stereo" \
    -c:a aac -b:a 192k "$MEDIA_DIR/music.m4a"

echo "== building release binary and demo generator =="
cargo build --release --bins --examples

echo "== importing media and saving $PROJECT =="
"$ROOT/target/release/examples/make_demo" "$MEDIA_DIR" "$PROJECT"

echo "== exporting 1080p H.264/AAC with the CLI =="
"$ROOT/target/release/splice" export \
    --project "$PROJECT" --preset 1080p --output "$OUTPUT"

ffprobe -v error -select_streams v:0 \
    -show_entries stream=codec_name,width,height,r_frame_rate \
    -of default=noprint_wrappers=1 "$OUTPUT"
ffprobe -v error -select_streams a:0 \
    -show_entries stream=codec_name,sample_rate,channels \
    -of default=noprint_wrappers=1 "$OUTPUT"

if [[ "${SKIP_SCREENSHOT:-0}" == "1" ]]; then
    echo "SKIP_SCREENSHOT=1; GUI screenshot not captured"
else
    if [[ -z "${WAYLAND_DISPLAY:-}" && -z "${DISPLAY:-}" ]]; then
        echo "demo: no Wayland/X11 display; set SKIP_SCREENSHOT=1 to run headless" >&2
        exit 1
    fi
    echo "== capturing the real GUI to $SCREENSHOT =="
    "$ROOT/target/release/splice" "$PROJECT" \
        --playhead 2.5 --screenshot "$SCREENSHOT" --exit-after-screenshot

    test -s "$SCREENSHOT" || {
        echo "demo: screenshot was not written" >&2
        exit 1
    }
    echo "SCREENSHOT_OK $SCREENSHOT"
fi

echo "DEMO_OK project=$PROJECT video=$OUTPUT screenshot=${SCREENSHOT}"
