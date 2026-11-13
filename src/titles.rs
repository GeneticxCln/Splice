use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result};

use crate::model::{
    Clip, MotionDirection, MotionEasing, Project, Title, TitleMotion, TitleMotionPreset, TitleStyle,
};

const SHORTS_SAFE_SIDE: f32 = 0.15;
const SHORTS_SAFE_TOP: f32 = 0.10;
const SHORTS_SAFE_BOTTOM: f32 = 0.20;

#[derive(Debug, Clone, Copy, PartialEq)]
struct SafeArea {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl SafeArea {
    fn shorts(width: u32, height: u32) -> Self {
        let width = width as f32;
        let height = height as f32;
        Self {
            left: width * SHORTS_SAFE_SIDE,
            top: height * SHORTS_SAFE_TOP,
            right: width * (1.0 - SHORTS_SAFE_SIDE),
            bottom: height * (1.0 - SHORTS_SAFE_BOTTOM),
        }
    }

    fn width(self) -> f32 {
        self.right - self.left
    }

    fn height(self) -> f32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone)]
struct TitleLayout {
    style_font_size: f32,
    event_font_size: f32,
    text: String,
    max_line_width: f32,
    block_height: f32,
    motion_scale: f32,
    safe: Option<SafeArea>,
}

struct WrappedText {
    text: String,
    max_line_width: f32,
    block_height: f32,
}

/// Builds an ASS subtitle file for every title clip in the project. ASS is
/// rendered by ffmpeg's libass filter and is the single title renderer for both
/// preview and export, so the two paths cannot drift apart.
pub fn write_ass(project: &Project, width: u32, height: u32, path: &Path) -> Result<bool> {
    let titles = project.title_clips();
    if titles.is_empty() {
        return Ok(false);
    }
    let layouts: Vec<TitleLayout> = titles
        .iter()
        .filter_map(|clip| clip.title.as_ref())
        .map(|title| layout_title(title, width, height))
        .collect();

    let mut ass = String::new();
    writeln!(ass, "[Script Info]")?;
    writeln!(ass, "ScriptType: v4.00+")?;
    writeln!(ass, "PlayResX: {width}")?;
    writeln!(ass, "PlayResY: {height}")?;
    writeln!(ass, "WrapStyle: 2")?;
    writeln!(ass, "ScaledBorderAndShadow: yes")?;
    writeln!(ass, "YCbCr Matrix: TV.709")?;
    writeln!(ass)?;
    writeln!(ass, "[V4+ Styles]")?;
    writeln!(
        ass,
        "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding"
    )?;

    for (index, clip) in titles.iter().enumerate() {
        let Some(title) = &clip.title else {
            continue;
        };
        let style = style_line(index, title, &layouts[index]);
        writeln!(ass, "{style}")?;
    }

    writeln!(ass)?;
    writeln!(ass, "[Events]")?;
    writeln!(
        ass,
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text"
    )?;

    for (index, clip) in titles.iter().enumerate() {
        let Some(title) = &clip.title else {
            continue;
        };
        let event = event_line(index, clip, title, &layouts[index], width, height);
        writeln!(ass, "{event}")?;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    std::fs::write(path, ass)
        .with_context(|| format!("could not write title overlay {}", path.display()))?;
    Ok(true)
}

/// A font name goes into a comma-separated ASS style line, so commas and
/// control characters (which could start a new line) are replaced.
fn sanitize_font_name(font: &str) -> String {
    font.chars()
        .map(|character| {
            if character == ',' || character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn style_line(index: usize, title: &Title, layout: &TitleLayout) -> String {
    let font_size = layout.style_font_size;
    let font = if title.font.trim().is_empty() {
        "DejaVu Sans"
    } else {
        title.font.as_str()
    };
    let font = sanitize_font_name(font);
    let primary = ass_color(&title.color, 0);
    let outline = ass_color(&title.background, alpha_for(title.background_opacity));
    let back = ass_color(&title.background, alpha_for(title.background_opacity));
    let (border_style, outline_width) = if title.background_opacity > 0.01 {
        (3, (font_size * 0.24).round().max(6.0))
    } else {
        (1, 0.0)
    };
    let bold = if title.bold { -1 } else { 0 };
    format!(
        "Style: Title{index},{font},{font_size:.0},{primary},&H000000FF,{outline},{back},{bold},0,0,0,100,100,0,0,{border_style},{outline_width:.0},0,5,0,0,0,1"
    )
}

fn event_line(
    index: usize,
    clip: &Clip,
    title: &Title,
    layout: &TitleLayout,
    width: u32,
    height: u32,
) -> String {
    if let Some(motion) = &title.motion {
        return motion_event_line(index, clip, title, layout, motion, width, height);
    }

    let duration = clip.duration().max(0.05);
    let start = fmt_time(clip.start.max(0.0));
    let end = fmt_time(clip.start.max(0.0) + duration);
    let size = layout.event_font_size;
    let (x, y) = title_position(title, layout, width, height);
    let intro = (duration.min(0.55) * 0.8 * 1000.0) as i64;
    let outro = (duration.min(0.55) * 0.8 * 1000.0) as i64;
    let dur_ms = (duration * 1000.0) as i64;

    let positioned = matches!(
        title.style,
        TitleStyle::Fade
            | TitleStyle::Pop
            | TitleStyle::BlurIn
            | TitleStyle::Tracking
            | TitleStyle::SpinIn
            | TitleStyle::Pulse
    );
    let mut tags = if positioned {
        format!("\\an5\\pos({x:.0},{y:.0})")
    } else {
        String::new()
    };
    match title.style {
        TitleStyle::Fade => {
            let _ = write!(tags, "\\fad({intro},{outro})");
        }
        TitleStyle::Rise => {
            let _ = write!(
                tags,
                "\\an5\\move({x:.0},{:.0},{x:.0},{y:.0},0,520)\\fad({intro},{outro})",
                y + size * 1.1
            );
        }
        TitleStyle::SlideLeft => {
            let _ = write!(
                tags,
                "\\an5\\move({:.0},{y:.0},{x:.0},{y:.0},0,520)\\fad({intro},{outro})",
                x - width as f32 * 0.12
            );
        }
        TitleStyle::Pop => {
            let _ = write!(
                tags,
                "\\fad(120,{outro})\\fscx38\\fscy38\\t(0,220,\\fscx106\\fscy106)\\t(220,400,\\fscx100\\fscy100)"
            );
        }
        TitleStyle::BlurIn => {
            let _ = write!(tags, "\\blur18\\t(0,480,\\blur0)\\fad({intro},{outro})");
        }
        TitleStyle::Tracking => {
            let _ = write!(
                tags,
                "\\fsp{:.0}\\fad({intro},{outro})\\t(0,700,\\fsp{:.0})",
                size * 0.35,
                size * 0.02
            );
        }
        TitleStyle::LowerThird => {
            let (start_x, lower_y) = if layout.safe.is_some() {
                (x, y)
            } else {
                ((width as f32 * 0.04).max(40.0), height as f32 * 0.84)
            };
            let _ = write!(
                tags,
                "\\an4\\move({start_x:.0},{:.0},{start_x:.0},{:.0},0,480)\\fad({intro},{outro})",
                lower_y + 85.0,
                lower_y
            );
        }
        TitleStyle::CreditRoll => {
            let _ = write!(
                tags,
                "\\an5\\move({x:.0},{:.0},{x:.0},{:.0},0,{dur_ms})",
                height as f32 * 1.08,
                -((height as f32) * 0.12)
            );
        }
        TitleStyle::SpinIn => {
            let _ = write!(
                tags,
                "\\frz-14\\fscx62\\fscy62\\t(0,520,\\frz0\\fscx100\\fscy100)\\fad({intro},{outro})"
            );
        }
        TitleStyle::Pulse => {
            let _ = write!(
                tags,
                "\\fad(300,{outro})\\t(0,900,\\fscx104\\fscy104)\\t(900,1800,\\fscx100\\fscy100)\\t(1800,2700,\\fscx104\\fscy104)\\t(2700,3600,\\fscx100\\fscy100)"
            );
        }
    }

    let text = &layout.text;

    format!("Dialogue: 0,{start},{end},Title{index},,0,0,0,,{{{tags}}}{text}")
}

fn motion_event_line(
    index: usize,
    clip: &Clip,
    title: &Title,
    layout: &TitleLayout,
    motion: &TitleMotion,
    width: u32,
    height: u32,
) -> String {
    let duration = clip.duration().max(0.05);
    let start = fmt_time(clip.start.max(0.0));
    let end = fmt_time(clip.start.max(0.0) + duration);
    let (x, y) = title_position(title, layout, width, height);
    let in_ms = (motion.in_duration.clamp(0.0, duration) * 1000.0).round() as i64;
    let out_ms = (motion.out_duration.clamp(0.0, duration) * 1000.0).round() as i64;
    let size = layout.event_font_size;
    let distance_x = width as f32 * motion.distance.clamp(0.0, 0.5) as f32;
    let distance_y = height as f32 * motion.distance.clamp(0.0, 0.5) as f32;
    let mut tags = format!("\\an5\\pos({x:.0},{y:.0})");
    let mut text = layout.text.clone();

    match motion.preset {
        TitleMotionPreset::Fade => {
            let _ = write!(tags, "\\fad({in_ms},{out_ms})");
        }
        TitleMotionPreset::Rise => {
            let _ = write!(
                tags,
                "\\move({x:.0},{:.0},{x:.0},{y:.0},0,{in_ms})\\fad({in_ms},{out_ms})",
                y + distance_y
            );
        }
        TitleMotionPreset::Slide => {
            let (start_x, start_y) = match motion.direction {
                MotionDirection::Left => (x + distance_x, y),
                MotionDirection::Right => (x - distance_x, y),
                MotionDirection::Up => (x, y + distance_y),
                MotionDirection::Down => (x, y - distance_y),
            };
            let _ = write!(
                tags,
                "\\move({start_x:.0},{start_y:.0},{x:.0},{y:.0},0,{in_ms})\\fad({in_ms},{out_ms})"
            );
        }
        TitleMotionPreset::Pop => {
            let settle = (in_ms * 55 / 100).max(1);
            let _ = write!(
                tags,
                "\\fad(120,{out_ms})\\fscx38\\fscy38{}\\t({settle},{in_ms},\\fscx100\\fscy100)",
                transform_tag(0, settle, motion.easing, "\\fscx106\\fscy106")
            );
        }
        TitleMotionPreset::Blur => {
            let _ = write!(
                tags,
                "\\blur18{}\\fad({in_ms},{out_ms})",
                transform_tag(0, in_ms, motion.easing, "\\blur0")
            );
        }
        TitleMotionPreset::Tracking => {
            let _ = write!(
                tags,
                "\\fsp{:.0}\\fad({in_ms},{out_ms}){}",
                size * 0.35,
                transform_tag(0, in_ms, motion.easing, &format!("\\fsp{:.0}", size * 0.02))
            );
        }
        TitleMotionPreset::Typewriter => {
            let _ = write!(tags, "\\fad(0,{out_ms})");
            text = typewriter_text(&layout.text, in_ms);
        }
    }

    format!("Dialogue: 0,{start},{end},Title{index},,0,0,0,,{{{tags}}}{text}")
}

fn transform_tag(start: i64, end: i64, easing: MotionEasing, modifiers: &str) -> String {
    let accel = match easing {
        MotionEasing::Linear => 1.0,
        MotionEasing::EaseIn => 2.0,
        MotionEasing::EaseOut => 0.5,
        MotionEasing::EaseInOut => 1.0,
    };
    format!("\\t({start},{end},{accel:.1},{modifiers})")
}

/// Wraps visible characters in an alpha reveal while leaving ASS control
/// sequences (line breaks and style overrides) intact.
fn typewriter_text(text: &str, duration_ms: i64) -> String {
    let visible = visible_character_count(text).max(1) as i64;
    let step = (duration_ms / visible).max(8);
    let mut result = String::new();
    let mut visible_index = 0i64;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\' {
            result.push(character);
            if let Some(next) = chars.next() {
                result.push(next);
            }
            continue;
        }
        if character == '{' {
            result.push(character);
            for next in chars.by_ref() {
                result.push(next);
                if next == '}' {
                    break;
                }
            }
            continue;
        }
        if character == '}' {
            result.push(character);
            continue;
        }
        let start = visible_index * step;
        let end = (start + step).min(duration_ms.max(step));
        if character.is_whitespace() {
            result.push(character);
            visible_index += 1;
            continue;
        }
        let _ = write!(
            result,
            "{{\\alpha&HFF&\\t({start},{end},\\alpha&H00&)}}{character}"
        );
        visible_index += 1;
    }
    result
}

fn visible_character_count(text: &str) -> usize {
    let mut count = 0usize;
    let mut in_override = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '{' {
            in_override = true;
            continue;
        }
        if character == '}' {
            in_override = false;
            continue;
        }
        if in_override {
            continue;
        }
        if character == '\\' {
            chars.next();
            continue;
        }
        count += 1;
    }
    count
}

fn layout_title(title: &Title, width: u32, height: u32) -> TitleLayout {
    let scale = font_scale(width, height);
    let base_size = (title.size.max(8.0) * scale).round().max(8.0);
    if width >= height {
        return TitleLayout {
            style_font_size: base_size,
            event_font_size: (title.size.max(8.0) * scale).max(8.0),
            text: title_text(title, (title.size.max(8.0) * scale).max(8.0) * 0.52, false),
            max_line_width: 0.0,
            block_height: 0.0,
            motion_scale: 1.0,
            safe: None,
        };
    }

    let safe = SafeArea::shorts(width, height);
    let motion = motion_scale(title);
    let mut font_size = base_size;
    let mut wrapped = wrap_title(title, safe.width(), font_size);
    // Long Shorts titles shrink only as much as needed to keep the complete
    // block inside the title-safe column and above the platform overlays.
    for _ in 0..12 {
        if wrapped.max_line_width * motion <= safe.width() + 0.5
            && wrapped.block_height * motion <= safe.height() + 0.5
        {
            break;
        }
        if font_size <= 8.0 {
            break;
        }
        font_size = (font_size * 0.9).max(8.0);
        wrapped = wrap_title(title, safe.width(), font_size);
    }

    TitleLayout {
        style_font_size: font_size,
        event_font_size: font_size,
        text: wrapped.text,
        max_line_width: wrapped.max_line_width,
        block_height: wrapped.block_height,
        motion_scale: motion,
        safe: Some(safe),
    }
}

fn title_text(title: &Title, subtitle_size: f32, reset_spacing: bool) -> String {
    let mut text = sanitize_text(&title.text);
    if !title.subtitle.trim().is_empty() {
        let reset = if reset_spacing { "\\fsp0" } else { "" };
        let _ = write!(
            text,
            "\\N{{\\fs{subtitle_size:.0}\\alpha&H20&{reset}}}{}",
            sanitize_text(&title.subtitle)
        );
    }
    text
}

fn wrap_title(title: &Title, max_width: f32, font_size: f32) -> WrappedText {
    let custom_tracking = title
        .motion
        .as_ref()
        .map(|motion| motion.preset == TitleMotionPreset::Tracking)
        .unwrap_or(false);
    let spacing = if title.style == TitleStyle::Tracking || custom_tracking {
        font_size * 0.35
    } else {
        0.0
    };
    let title_lines = wrap_lines(&title.text, max_width, font_size, title.bold, spacing);
    let subtitle_size = font_size * 0.52;
    let subtitle_lines = if title.subtitle.trim().is_empty() {
        Vec::new()
    } else {
        wrap_lines(&title.subtitle, max_width, subtitle_size, title.bold, 0.0)
    };

    let mut text = title_lines.join("\\N");
    if !subtitle_lines.is_empty() {
        if !text.is_empty() {
            text.push_str("\\N");
        }
        let _ = write!(
            text,
            "{{\\fs{subtitle_size:.0}\\alpha&H20&\\fsp0}}{}",
            subtitle_lines.join("\\N")
        );
    }

    let title_width = title_lines
        .iter()
        .map(|line| line_width(line, font_size, title.bold, spacing))
        .fold(0.0_f32, f32::max);
    let subtitle_width = subtitle_lines
        .iter()
        .map(|line| line_width(line, subtitle_size, title.bold, 0.0))
        .fold(0.0_f32, f32::max);
    let block_height = title_lines.len() as f32 * font_size * 1.2
        + subtitle_lines.len() as f32 * subtitle_size * 1.2;

    WrappedText {
        text,
        max_line_width: title_width.max(subtitle_width),
        block_height,
    }
}

fn wrap_lines(text: &str, max_width: f32, font_size: f32, bold: bool, spacing: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in text.split('\n') {
        let clean = sanitize_text(raw_line);
        lines.extend(wrap_paragraph(&clean, max_width, font_size, bold, spacing));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn wrap_paragraph(
    paragraph: &str,
    max_width: f32,
    font_size: f32,
    bold: bool,
    spacing: f32,
) -> Vec<String> {
    let words: Vec<&str> = paragraph.split_whitespace().collect();
    if words.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words {
        if line_width(word, font_size, bold, spacing) > max_width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let chunks = break_word(word, max_width, font_size, bold, spacing);
            let last = chunks.len().saturating_sub(1);
            for (index, chunk) in chunks.into_iter().enumerate() {
                if index == last {
                    current = chunk;
                } else {
                    lines.push(chunk);
                }
            }
            continue;
        }

        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if current.is_empty() || line_width(&candidate, font_size, bold, spacing) <= max_width {
            current = candidate;
        } else {
            lines.push(std::mem::replace(&mut current, word.to_string()));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn break_word(word: &str, max_width: f32, font_size: f32, bold: bool, spacing: f32) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for character in word.chars() {
        let mut candidate = current.clone();
        candidate.push(character);
        if !current.is_empty() && line_width(&candidate, font_size, bold, spacing) > max_width {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(character);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn line_width(line: &str, font_size: f32, bold: bool, spacing: f32) -> f32 {
    let mut characters = 0usize;
    let mut em = 0.0_f32;
    for character in line.chars() {
        characters += 1;
        em += glyph_em(character);
    }
    if characters == 0 {
        return 0.0;
    }
    let weight = if bold { 1.04 } else { 1.0 };
    em * font_size * weight + spacing * characters as f32
}

fn glyph_em(character: char) -> f32 {
    match character {
        ' ' | '\t' => 0.30,
        '\'' | '"' | '`' | '.' | ',' | ':' | ';' | '!' | '|' => 0.30,
        'i' | 'l' | 'I' | 'j' | 't' | 'f' | 'r' => 0.36,
        'm' | 'w' | 'M' | 'W' => 0.82,
        'A'..='Z' => 0.68,
        '0'..='9' => 0.58,
        character if character.is_ascii() => 0.56,
        _ => 1.0,
    }
}

fn motion_scale(title: &Title) -> f32 {
    if let Some(motion) = &title.motion {
        return match motion.preset {
            TitleMotionPreset::Pop => 1.06,
            _ => 1.0,
        };
    }
    match title.style {
        TitleStyle::Pop => 1.06,
        TitleStyle::Pulse => 1.04,
        TitleStyle::SpinIn => 1.12,
        _ => 1.0,
    }
}

fn title_position(title: &Title, layout: &TitleLayout, width: u32, height: u32) -> (f32, f32) {
    let requested_x = (title.position_x.clamp(0.0, 1.0) * width as f32).round();
    let requested_y = (title.position_y.clamp(0.0, 1.0) * height as f32).round();
    let Some(safe) = layout.safe else {
        return (requested_x, requested_y);
    };

    let half_width = (layout.max_line_width * layout.motion_scale * 0.5).min(safe.width() * 0.5);
    let half_height = (layout.block_height * layout.motion_scale * 0.5).min(safe.height() * 0.5);

    if title.style == TitleStyle::LowerThird {
        return (
            safe.left.round(),
            clamp_center(height as f32 * 0.72, safe.top, safe.bottom, half_height),
        );
    }
    if title.style == TitleStyle::CreditRoll {
        return (
            clamp_center(requested_x, safe.left, safe.right, half_width),
            requested_y,
        );
    }

    (
        clamp_center(requested_x, safe.left, safe.right, half_width),
        clamp_center(requested_y, safe.top, safe.bottom, half_height),
    )
}

fn clamp_center(value: f32, min: f32, max: f32, half: f32) -> f32 {
    let low = (min + half).min(max - half);
    let high = (max - half).max(min + half);
    value.clamp(low, high).round()
}

/// Returns a warning when fontconfig resolves `font` to a different family,
/// which means libass will silently substitute it during preview and export.
/// `None` means the font is available or fontconfig is unavailable.
pub fn font_fallback_warning(font: &str) -> Option<String> {
    let font = font.trim();
    if font.is_empty() {
        return None;
    }
    static CACHE: OnceLock<Mutex<HashMap<String, Option<bool>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cached = cache.lock().ok().and_then(|cache| cache.get(font).copied());
    let available = match cached {
        Some(value) => value,
        None => {
            let value = font_matches(font);
            if let Ok(mut cache) = cache.lock() {
                cache.insert(font.to_string(), value);
            }
            value
        }
    };
    match available {
        Some(false) => Some(format!(
            "“{font}” is not installed; libass will substitute a fallback font."
        )),
        Some(true) | None => None,
    }
}

fn font_matches(font: &str) -> Option<bool> {
    let output = Command::new("fc-match")
        .args(["-f", "%{family}"])
        .arg(font)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let families = String::from_utf8_lossy(&output.stdout);
    Some(
        families
            .split(',')
            .any(|family| family.trim().eq_ignore_ascii_case(font)),
    )
}

fn sanitize_text(text: &str) -> String {
    text.replace('\\', "/")
        .replace('\r', "")
        .replace('\n', "\\N")
        .replace('{', "(")
        .replace('}', ")")
}

fn font_scale(width: u32, height: u32) -> f32 {
    width.min(height) as f32 / 1080.0
}

fn alpha_for(opacity: f32) -> u8 {
    ((1.0 - opacity.clamp(0.0, 1.0)) * 255.0).round() as u8
}

fn ass_color(hex: &str, alpha: u8) -> String {
    let clean = hex.trim().trim_start_matches('#');
    let parsed = u32::from_str_radix(clean, 16).ok();
    let (r, g, b) = match parsed {
        Some(value) if clean.len() >= 6 => (
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        ),
        _ => (245, 242, 234),
    };
    format!("&H{alpha:02X}{b:02X}{g:02X}{r:02X}")
}

fn fmt_time(seconds: f64) -> String {
    let total_centis = (seconds.max(0.0) * 100.0).round() as u64;
    let centis = total_centis % 100;
    let total_seconds = total_centis / 100;
    let secs = total_seconds % 60;
    let minutes = (total_seconds / 60) % 60;
    let hours = total_seconds / 3600;
    format!("{hours}:{minutes:02}:{secs:02}.{centis:02}")
}

pub fn escape_filter_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    #[test]
    fn font_names_cannot_inject_extra_ass_lines() {
        let mut title = Title::opening("Hello");
        title.font =
            "Evil,1\n[Events]\nDialogue: 0,0:00:00.00,9:00:00.00,Default,,0,0,0,,pwned".into();
        let layout = layout_title(&title, 1920, 1080);
        let line = style_line(0, &title, &layout);
        assert!(!line.contains('\n'), "{line:?}");
        assert_eq!(line.matches("Style:").count(), 1);
        assert!(!line.contains("Evil,"));
    }

    use super::*;

    #[test]
    fn font_scale_uses_the_short_side() {
        assert!((font_scale(1920, 1080) - 1.0).abs() < 1e-6);
        assert!((font_scale(1080, 1920) - 1.0).abs() < 1e-6);
        assert!((font_scale(1280, 720) - (2.0 / 3.0)).abs() < 1e-6);
    }

    #[test]
    fn portrait_titles_wrap_inside_the_safe_column() {
        let mut title = Title::opening("Long Short");
        title.text =
            "AUTOMATIC TITLE WRAPPING KEEPS LONG HEADLINES INSIDE THE VERTICAL SAFE COLUMN".into();
        title.subtitle = "Subtitles use the same safe width and remain readable".into();
        title.position_x = 0.0;
        title.position_y = 0.0;

        let layout = layout_title(&title, 1080, 1920);
        let safe = layout.safe.expect("portrait layout must have a safe area");
        assert!(layout.text.contains("\\N"), "long text must wrap");
        assert!(
            layout.max_line_width * layout.motion_scale <= safe.width() + 1.0,
            "wrapped lines must fit the safe column"
        );
        assert!(
            layout.block_height * layout.motion_scale <= safe.height() + 1.0,
            "the title block must fit the safe height"
        );

        let (x, y) = title_position(&title, &layout, 1080, 1920);
        let half_width = layout.max_line_width * layout.motion_scale * 0.5;
        let half_height = layout.block_height * layout.motion_scale * 0.5;
        assert!(x - half_width >= safe.left - 1.0);
        assert!(x + half_width <= safe.right + 1.0);
        assert!(y - half_height >= safe.top - 1.0);
        assert!(y + half_height <= safe.bottom + 1.0);
    }

    #[test]
    fn portrait_lower_third_starts_at_the_safe_left_edge() {
        let mut title = Title::opening("Lower Third");
        title.style = TitleStyle::LowerThird;
        title.text = "A LOWER THIRD WITH ENOUGH TEXT TO WRAP SAFELY".into();
        title.position_x = 0.95;
        title.position_y = 0.99;

        let layout = layout_title(&title, 1080, 1920);
        let safe = layout.safe.expect("portrait layout must have a safe area");
        let (x, y) = title_position(&title, &layout, 1080, 1920);
        let half_height = layout.block_height * layout.motion_scale * 0.5;
        assert!((x - safe.left).abs() < 1.0);
        assert!(y + half_height <= safe.bottom + 1.0);
    }

    #[test]
    fn landscape_titles_keep_the_legacy_layout() {
        let mut title = Title::opening("Landscape");
        title.text = "THIS LANDSCAPE TITLE IS LONG BUT MUST NOT WRAP".into();
        title.subtitle.clear();
        title.position_x = 0.2;
        title.position_y = 0.7;

        let layout = layout_title(&title, 1920, 1080);
        assert!(layout.safe.is_none());
        assert_eq!(layout.text, title.text);
        assert_eq!(layout.style_font_size, 96.0);
        assert_eq!(layout.event_font_size, 96.0);
        assert_eq!(title_position(&title, &layout, 1920, 1080), (384.0, 756.0));
    }

    #[test]
    fn time_format_is_centisecond_accurate() {
        assert_eq!(fmt_time(0.0), "0:00:00.00");
        assert_eq!(fmt_time(61.234), "0:01:01.23");
        assert_eq!(fmt_time(3661.5), "1:01:01.50");
    }

    #[test]
    fn ass_color_is_bgr_with_alpha() {
        assert_eq!(ass_color("#FF6B4A", 0), "&H004A6BFF");
        assert_eq!(ass_color("#000000", 128), "&H80000000");
    }

    #[test]
    fn text_cannot_inject_override_tags() {
        assert_eq!(sanitize_text("hi {\\b1}"), "hi (/b1)");
    }

    #[test]
    fn custom_typewriter_motion_emits_alpha_reveal() {
        let mut project = Project::new("Motion");
        let mut title = Title::opening("Hello");
        title.motion = Some(TitleMotion {
            preset: TitleMotionPreset::Typewriter,
            in_duration: 0.8,
            ..TitleMotion::default()
        });
        project.add_title(title, 0.0, 2.0);
        let clip = project.title_clips()[0];
        let title = clip.title.as_ref().expect("title");
        let layout = layout_title(title, 1920, 1080);
        let event = event_line(0, clip, title, &layout, 1920, 1080);

        assert!(event.contains("\\alpha&HFF&"));
        assert!(event.contains("\\t("));
        assert!(event.contains("}H"));
        assert!(event.contains("}o"));
    }

    #[test]
    fn legacy_titles_keep_their_style_animation() {
        let mut project = Project::new("Legacy");
        project.add_title(Title::opening("Hello"), 0.0, 2.0);
        let clip = project.title_clips()[0];
        let title = clip.title.as_ref().expect("title");
        let layout = layout_title(title, 1920, 1080);
        let event = event_line(0, clip, title, &layout, 1920, 1080);

        assert!(event.contains("\\fad("));
        assert!(!event.contains("\\alpha&HFF&"));
    }

    #[test]
    fn typewriter_keeps_line_breaks_and_subtitle_tags() {
        let text = typewriter_text("A\\N{\\fs20}BC", 200);
        assert!(text.contains("\\N"));
        assert!(text.contains("{\\fs20}"));
        assert_eq!(text.matches("\\alpha&HFF&").count(), 3);
    }

    #[test]
    fn missing_font_reports_a_fallback_warning() {
        if Command::new("fc-match").arg("--version").output().is_err() {
            return;
        }
        assert!(
            font_fallback_warning("Splice Missing Font 12345").is_some(),
            "an unavailable family should be reported to the editor"
        );
    }
}
