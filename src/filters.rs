use std::fmt::Write as _;

use crate::model::{EffectInstance, EffectKind, FPS, Framing};

/// Appends the ffmpeg filter for one effect to `chain` (each filter is
/// prefixed with a comma). Parameters are clamped to ranges ffmpeg accepts.
/// `requested` is the clip length in seconds and `width`/`height` the render
/// size; only effects that animate over the clip need them.
pub fn push_effect(
    chain: &mut String,
    effect: &EffectInstance,
    requested: f64,
    width: u32,
    height: u32,
) {
    let param = |key: &str, fallback: f64| effect.params.get(key).copied().unwrap_or(fallback);
    match effect.kind {
        EffectKind::Brightness => {
            let _ = write!(chain, ",eq=brightness={:.4}", param("brightness", 0.08));
        }
        EffectKind::Contrast => {
            let _ = write!(chain, ",eq=contrast={:.4}", param("contrast", 1.18));
        }
        EffectKind::Saturation => {
            let _ = write!(chain, ",eq=saturation={:.4}", param("saturation", 1.25));
        }
        EffectKind::HueRotate => {
            let _ = write!(chain, ",hue=h={:.3}", param("degrees", 18.0));
        }
        EffectKind::Monochrome => {
            let amount = param("amount", 1.0).clamp(0.0, 1.0);
            let _ = write!(chain, ",hue=s={:.4}", 1.0 - amount);
        }
        EffectKind::Vignette => {
            let strength = param("strength", 0.55).clamp(0.1, 1.0);
            let angle = (std::f64::consts::PI / 5.0) * (0.4 + strength);
            let _ = write!(chain, ",vignette=angle={angle:.4}");
        }
        EffectKind::Sharpen => {
            let amount = param("amount", 0.8).clamp(0.0, 2.0);
            let _ = write!(chain, ",unsharp=5:5:{amount:.3}:5:5:0");
        }
        EffectKind::Blur => {
            let sigma = param("sigma", 6.0).clamp(0.3, 24.0);
            let _ = write!(chain, ",gblur=sigma={sigma:.3}");
        }
        EffectKind::Temperature => {
            let kelvin = param("temperature", 7400.0).clamp(3000.0, 10000.0);
            let _ = write!(chain, ",colortemperature=temperature={kelvin:.0}");
        }
        EffectKind::Gamma => {
            let gamma = param("gamma", 1.1).clamp(0.1, 4.0);
            let _ = write!(chain, ",eq=gamma={gamma:.4}");
        }
        EffectKind::ColorBalance => {
            let red = param("red", 0.0).clamp(-1.0, 1.0);
            let green = param("green", 0.0).clamp(-1.0, 1.0);
            let blue = param("blue", 0.0).clamp(-1.0, 1.0);
            let _ = write!(
                chain,
                ",colorbalance=rs={red:.4}:gs={green:.4}:bs={blue:.4}:rm={red:.4}:gm={green:.4}:bm={blue:.4}:rh={red:.4}:gh={green:.4}:bh={blue:.4}"
            );
        }
        EffectKind::Vibrance => {
            let intensity = param("intensity", 0.35).clamp(-2.0, 2.0);
            let _ = write!(chain, ",vibrance=intensity={intensity:.4}");
        }
        EffectKind::HueSaturation => {
            let hue = param("hue", 0.0).clamp(-180.0, 180.0);
            let saturation = param("saturation", 0.15).clamp(-1.0, 1.0);
            let intensity = param("intensity", 0.0).clamp(-1.0, 1.0);
            let _ = write!(
                chain,
                ",huesaturation=hue={hue:.3}:saturation={saturation:.4}:intensity={intensity:.4}"
            );
        }
        EffectKind::Colorize => {
            let hue = param("hue", 210.0).clamp(0.0, 360.0);
            let saturation = param("saturation", 0.35).clamp(0.0, 1.0);
            let lightness = param("lightness", 0.5).clamp(0.0, 1.0);
            let mix = param("mix", 0.6).clamp(0.0, 1.0);
            let _ = write!(
                chain,
                ",colorize=hue={hue:.2}:saturation={saturation:.4}:lightness={lightness:.4}:mix={mix:.4}"
            );
        }
        EffectKind::Invert => {
            let amount = param("amount", 1.0).clamp(0.0, 1.0);
            let coefficient = 1.0 - 2.0 * amount;
            let _ = write!(
                chain,
                ",colorchannelmixer=rr={coefficient:.4}:gg={coefficient:.4}:bb={coefficient:.4}:ra={amount:.4}:ga={amount:.4}:ba={amount:.4}"
            );
        }
        EffectKind::EdgeDetect => {
            let low = param("low", 0.08).clamp(0.0, 1.0);
            let high = param("high", 0.2).clamp(low, 1.0);
            let _ = write!(chain, ",edgedetect=low={low:.4}:high={high:.4}:mode=wires");
        }
        EffectKind::FilmGrain => {
            let strength = param("strength", 12.0).clamp(0.0, 100.0).round() as i32;
            let _ = write!(chain, ",noise=alls={strength}:allf=t+u");
        }
        EffectKind::Posterize => {
            let levels = param("levels", 8.0).clamp(2.0, 64.0).round() as i32;
            let _ = write!(chain, ",elbg=codebook_length={levels}:nb_steps=1");
        }
        EffectKind::Median => {
            let radius = param("radius", 2.0).clamp(1.0, 9.0).round() as i32;
            let _ = write!(chain, ",median=radius={radius}");
        }
        EffectKind::SmartBlur => {
            let radius = param("radius", 1.5).clamp(0.1, 5.0);
            let strength = param("strength", 0.6).clamp(-1.0, 1.0);
            let _ = write!(chain, ",smartblur=lr={radius:.3}:ls={strength:.3}:lt=0");
        }
        EffectKind::BoxBlur => {
            let radius = param("radius", 3.0).clamp(1.0, 20.0).round() as i32;
            let power = param("power", 2.0).clamp(1.0, 4.0).round() as i32;
            let _ = write!(chain, ",boxblur=lr={radius}:lp={power}");
        }
        EffectKind::Bilateral => {
            let space = param("space", 8.0).clamp(0.0, 512.0);
            let range = param("range", 0.1).clamp(0.0, 1.0);
            let _ = write!(
                chain,
                ",bilateral=sigmaS={space:.3}:sigmaR={range:.4}:planes=15"
            );
        }
        EffectKind::Deband => {
            let threshold = param("threshold", 0.02).clamp(0.00003, 0.5);
            let range = param("range", 16.0).clamp(4.0, 64.0).round() as i32;
            let _ = write!(
                chain,
                ",deband=1thr={threshold:.5}:2thr={threshold:.5}:3thr={threshold:.5}:4thr={threshold:.5}:range={range}"
            );
        }
        EffectKind::Denoise => {
            let spatial = param("spatial", 2.0).clamp(0.0, 10.0);
            let temporal = param("temporal", 1.0).clamp(0.0, 10.0);
            let chroma_spatial = spatial * 0.75;
            let chroma_temporal = temporal * 0.75;
            let _ = write!(
                chain,
                ",hqdn3d=luma_spatial={spatial:.3}:chroma_spatial={chroma_spatial:.3}:luma_tmp={temporal:.3}:chroma_tmp={chroma_temporal:.3}"
            );
        }
        EffectKind::Deblock => {
            let block = param("block", 8.0).clamp(4.0, 512.0).round() as i32;
            let strength = param("strength", 0.2).clamp(0.0, 1.0);
            let beta = strength * 0.5;
            let gamma = strength * 0.5;
            let delta = strength * 0.5;
            let _ = write!(
                chain,
                ",deblock=filter=1:block={block}:alpha={strength:.4}:beta={beta:.4}:gamma={gamma:.4}:delta={delta:.4}"
            );
        }
        EffectKind::ChromaShift => {
            let horizontal = param("horizontal", 4.0).clamp(-255.0, 255.0).round() as i32;
            let vertical = param("vertical", 0.0).clamp(-255.0, 255.0).round() as i32;
            let _ = write!(
                chain,
                ",chromashift=cbh={horizontal}:crh={}:cbv={vertical}:crv={}",
                -horizontal, -vertical
            );
        }
        EffectKind::RgbSplit => {
            let offset = param("offset", 4.0).clamp(0.0, 255.0).round() as i32;
            let _ = write!(chain, ",rgbashift=rh={offset}:bh={}:rv=0:bv=0", -offset);
        }
        EffectKind::LensCorrection => {
            let k1 = param("k1", -0.12).clamp(-1.0, 1.0);
            let k2 = param("k2", 0.0).clamp(-1.0, 1.0);
            let _ = write!(chain, ",lenscorrection=k1={k1:.4}:k2={k2:.4}:i=bilinear");
        }
        EffectKind::Rotate => {
            let degrees = param("angle", 5.0).clamp(-45.0, 45.0);
            let radians = degrees.to_radians();
            let _ = write!(
                chain,
                ",rotate=angle={radians:.6}:fillcolor=black@0:ow=iw:oh=ih"
            );
        }
        EffectKind::FlipHorizontal => {
            let _ = write!(chain, ",hflip");
        }
        EffectKind::FlipVertical => {
            let _ = write!(chain, ",vflip");
        }
        EffectKind::Levels => {
            let black = param("black", 0.02).clamp(0.0, 0.4);
            let white = param("white", 0.98).clamp(black + 0.1, 1.0);
            let _ = write!(
                chain,
                ",colorlevels=rimin={black:.4}:gimin={black:.4}:bimin={black:.4}:rimax={white:.4}:gimax={white:.4}:bimax={white:.4}"
            );
        }
        EffectKind::FilmCurve => {
            let preset = param("preset", 10.0).clamp(0.0, 10.0).round() as i32;
            let _ = write!(chain, ",curves=preset={preset}");
        }
        EffectKind::HeatMap => {
            let preset = param("preset", 4.0).clamp(0.0, 20.0).round() as i32;
            let _ = write!(chain, ",pseudocolor=preset={preset}");
        }
        EffectKind::Pixelate => {
            let size = param("size", 12.0).clamp(1.0, 1024.0).round() as i32;
            let _ = write!(chain, ",pixelize=w={size}:h={size}");
        }
        EffectKind::ColorContrast => {
            let red_cyan = param("red_cyan", 0.1).clamp(-1.0, 1.0);
            let green_magenta = param("green_magenta", 0.0).clamp(-1.0, 1.0);
            let blue_yellow = param("blue_yellow", 0.0).clamp(-1.0, 1.0);
            let _ = write!(
                chain,
                ",colorcontrast=rc={red_cyan:.4}:gm={green_magenta:.4}:by={blue_yellow:.4}"
            );
        }
        EffectKind::ColorCorrect => {
            let shadows_red = param("shadows_red", 0.0).clamp(-1.0, 1.0);
            let shadows_blue = param("shadows_blue", 0.0).clamp(-1.0, 1.0);
            let highlights_red = param("highlights_red", 0.0).clamp(-1.0, 1.0);
            let highlights_blue = param("highlights_blue", 0.0).clamp(-1.0, 1.0);
            let saturation = param("saturation", 1.0).clamp(-3.0, 3.0);
            let _ = write!(
                chain,
                ",colorcorrect=rl={shadows_red:.4}:bl={shadows_blue:.4}:rh={highlights_red:.4}:bh={highlights_blue:.4}:saturation={saturation:.4}"
            );
        }
        EffectKind::PanZoom => {
            let zoom = param("zoom", 1.22).clamp(1.0, 2.0);
            let pan_x = param("pan_x", 0.0).clamp(-1.0, 1.0);
            let pan_y = param("pan_y", 0.0).clamp(-1.0, 1.0);
            let frames = (requested * FPS as f64).round().max(2.0);
            let _ = write!(
                chain,
                ",zoompan=z='min(1+({zoom:.4}-1)*on/{frames:.0},{zoom:.4})':x='iw/2-(iw/zoom/2)+({pan_x:.4})*iw*0.12':y='ih/2-(ih/zoom/2)+({pan_y:.4})*ih*0.12':d=1:s={width}x{height}:fps=30"
            );
        }
    }
}

pub fn frame_filter(width: u32, height: u32, framing: Framing, background: &str) -> String {
    match framing {
        Framing::Fit => format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color={}",
            ffmpeg_color(background)
        ),
        Framing::Fill => format!(
            "scale={width}:{height}:force_original_aspect_ratio=increase:force_divisible_by=2,crop={width}:{height}"
        ),
    }
}

pub fn ffmpeg_color(hex: &str) -> String {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 && clean.chars().all(|character| character.is_ascii_hexdigit()) {
        format!("0x{clean}")
    } else {
        "0x101318".to_string()
    }
}
