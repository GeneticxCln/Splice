# Splice project format (`.splice`)

Splice projects are UTF-8 JSON documents with the extension `.splice`. The
format is versioned so a future schema can migrate instead of guessing.

## Envelope

```json
{
  "format": "splice-project",
  "version": 1,
  "name": "My movie",
  "settings": {
    "width": 1920,
    "height": 1080,
    "fps": 30,
    "sample_rate": 48000,
    "background": "#0d1016",
    "framing": "fit"
  },
  "media": [],
  "tracks": []
}
```

| Field | Meaning |
| --- | --- |
| `format` | Always `splice-project`. Any other value is rejected. |
| `version` | Integer schema version. This build writes `1`. Files with a newer version are rejected with a clear error; version `0` is rejected because no migration exists. |
| `name` | Project name shown in the title bar. |
| `settings` | Canvas size, frame rate, sample rate, preview letterbox colour, and visual framing. |
| `media` | Probed source assets. Sources are referenced, never copied or rewritten. |
| `tracks` | Ordered video/overlay/audio tracks containing clips. |

## Media assets

```json
{
  "id": "media-18f...",
  "name": "clip-a",
  "path": "media/clip-a.mp4",
  "kind": "video",
  "duration": 6.0,
  "width": 1280,
  "height": 720,
  "has_audio": true,
  "sample_rate": 48000,
  "channels": 2
}
```

- `kind` is `video`, `image`, or `audio`.
- `path` is relative to the project file whenever the source lives inside the
  project directory. Sources outside it keep an absolute path as a fallback.
- A missing file is an offline asset: the media library and timeline show an
  offline placeholder, the preview marks it, and export is blocked with a
  non-zero exit and no output file.
- Relinking probes the replacement with ffprobe and validates that it is usable
  before updating the asset.

## Tracks and clips

A project has four default tracks: `Video 1` (primary), `Overlay`, `Music`, and
`Voiceover`. Track order is stored explicitly.

```json
{
  "id": "clip-18f...",
  "name": "clip-a",
  "media_id": "media-18f...",
  "start": 0.0,
  "in_point": 0.0,
  "out_point": 3.0,
  "effects": [
    {
      "id": "effect-18f...",
      "kind": "brightness",
      "params": { "amount": 0.08 }
    }
  ],
  "transition": { "kind": "dissolve", "duration": 0.5 },
  "audio": { "volume": 0.7, "fade_in": 0.3, "fade_out": 0.0, "muted": false },
  "title": null
}
```

- `start` is the timeline position in seconds.
- `in_point` / `out_point` trim the source non-destructively; `out_point` is
  exclusive in the render graph. The source file is never modified.
- Image clips have a settable `out_point`; their `media_id` still points at the
  still image.
- `effects` is an ordered list. `params` is a name → number map, so unknown
  parameters are preserved rather than silently dropped.
- `transition` belongs to the incoming clip. The timeline reflow places the clip
  so it overlaps the previous one by the transition duration.
- Title clips have `"media_id": null` and a `title` object:

```json
{
  "text": "Splice Demo Reel",
  "subtitle": "Rust + egui",
  "font": "DejaVu Sans",
  "size": 96.0,
  "color": "#F5F2EA",
  "background": "#0B1118",
  "background_opacity": 0.35,
  "position_x": 0.5,
  "position_y": 0.44,
  "style": "fade",
  "bold": true
}
```

- `style` is one of `fade`, `rise`, `slide_left`, `pop`, `blur_in`, `tracking`,
  `lower_third`, `credit_roll`, `spin_in`, `pulse`.
- `transition.kind` is one of the 58 ffmpeg `xfade` transitions: `fade`,
  `dissolve`, `wipe_left`, `wipe_right`, `wipe_up`, `wipe_down`, `slide_left`,
  `slide_right`, `slide_up`, `slide_down`, `circle_open`, `circle_close`,
  `circle_crop`, `rect_crop`, `radial`, `distance`, `pixelize`, `fade_black`,
  `fade_white`, `fade_grays`, `fade_fast`, `fade_slow`, `smooth_left`,
  `smooth_right`, `smooth_up`, `smooth_down`, `vert_open`, `vert_close`,
  `horz_open`, `horz_close`, `diag_tl`, `diag_tr`, `diag_bl`, `diag_br`,
  `wipe_tl`, `wipe_tr`, `wipe_bl`, `wipe_br`, `hl_slice`, `hr_slice`,
  `vu_slice`, `vd_slice`, `hl_wind`, `hr_wind`, `vu_wind`, `vd_wind`,
  `cover_left`, `cover_right`, `cover_up`, `cover_down`, `reveal_left`,
  `reveal_right`, `reveal_up`, `reveal_down`, `squeeze_h`, `squeeze_v`,
  `zoom_in`, `h_blur`.
- Effect `kind` is one of the 38 effects: `brightness`, `contrast`,
  `saturation`, `hue_rotate`, `monochrome`, `vignette`, `sharpen`, `blur`,
  `pan_zoom`, `temperature`, `gamma`, `color_balance`, `vibrance`,
  `hue_saturation`, `colorize`, `invert`, `edge_detect`, `film_grain`,
  `posterize`, `median`, `smart_blur`, `box_blur`, `bilateral`, `deband`,
  `denoise`, `deblock`, `chroma_shift`, `rgb_split`, `lens_correction`,
  `rotate`, `flip_horizontal`, `flip_vertical`, `levels`, `film_curve`,
  `heat_map`, `pixelate`, `color_contrast`, `color_correct`.

## Save and load guarantees

- Saving is atomic: the document is serialized to a temporary file in the same
  directory, fsynced, then renamed over the destination. A crash cannot leave a
  truncated project.
- Paths are normalized against the new project directory on every save, so a
  project moved together with its media keeps relative references.
- Loading validates the extension, UTF-8, JSON, envelope, version, settings, and
  project invariants (unique media ids, non-empty source paths, finite
  non-negative clip times, positive durations, and clips that are either media
  or title clips). Unknown enum values such as effects, transitions, or title
  styles are rejected by serde.
- A second save/load cycle is bit-stable; the acceptance suite asserts exact
  equality after the first round-trip normalizes one floating-point ULP.

## Compatibility policy

Adding an optional field with a serde default is a version-preserving change.
Any breaking change to meaning, removal, or required field must bump `version`
and add a migration in `project_io::migrate`. Saving always writes the current
version.

`settings.framing` is one such optional field. It is `fit` when absent, which
matches the original version 1 behaviour. `fill` scales visual clips to cover
the canvas and center-crops them. New 9:16 Short projects use `fill`; landscape
projects continue to use `fit`. No schema migration is required.
