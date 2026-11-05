use crate::model::{ClipMotion, MotionEasing, MotionKeyframe};

const EPSILON: f64 = 0.001;
const FPS: f64 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionValue {
    pub scale: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub rotation: f64,
    pub opacity: f64,
}

impl Default for MotionValue {
    fn default() -> Self {
        Self {
            scale: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            rotation: 0.0,
            opacity: 1.0,
        }
    }
}

impl MotionValue {
    pub fn from_keyframe(keyframe: &MotionKeyframe) -> Self {
        Self {
            scale: keyframe.scale,
            pan_x: keyframe.pan_x,
            pan_y: keyframe.pan_y,
            rotation: keyframe.rotation,
            opacity: keyframe.opacity,
        }
    }

    pub fn clamped(self) -> Self {
        Self {
            scale: self.scale.clamp(1.0, 2.0),
            pan_x: self.pan_x.clamp(-1.0, 1.0),
            pan_y: self.pan_y.clamp(-1.0, 1.0),
            rotation: self.rotation.clamp(-180.0, 180.0),
            opacity: self.opacity.clamp(0.0, 1.0),
        }
    }

    pub fn is_identity(self) -> bool {
        self.has_scale_pan_identity()
            && self.rotation.abs() < EPSILON
            && (self.opacity - 1.0).abs() < EPSILON
    }

    pub fn has_scale_pan_identity(self) -> bool {
        (self.scale - 1.0).abs() < EPSILON
            && self.pan_x.abs() < EPSILON
            && self.pan_y.abs() < EPSILON
    }
}

impl ClipMotion {
    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }

    pub fn sorted_keyframes(&self) -> Vec<&MotionKeyframe> {
        let mut keyframes: Vec<&MotionKeyframe> = self
            .keyframes
            .iter()
            .filter(|keyframe| {
                keyframe.time.is_finite()
                    && keyframe.scale.is_finite()
                    && keyframe.pan_x.is_finite()
                    && keyframe.pan_y.is_finite()
                    && keyframe.rotation.is_finite()
                    && keyframe.opacity.is_finite()
            })
            .collect();
        keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
        keyframes
    }

    /// Samples the curve at local seconds. `origin` maps the clip's current
    /// in-point onto the stable keyframe curve.
    pub fn sample(&self, time: f64) -> MotionValue {
        let time = time + self.origin;
        let keyframes = self.sorted_keyframes();
        let Some(first) = keyframes.first() else {
            return MotionValue::default();
        };
        if time <= first.time {
            return MotionValue::from_keyframe(first).clamped();
        }
        let Some(last) = keyframes.last() else {
            return MotionValue::default();
        };
        if time >= last.time {
            return MotionValue::from_keyframe(last).clamped();
        }

        for pair in keyframes.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if time <= b.time {
                let span = b.time - a.time;
                if span <= EPSILON {
                    return MotionValue::from_keyframe(b).clamped();
                }
                let t = ease((time - a.time) / span, self.easing);
                let start = MotionValue::from_keyframe(a);
                let end = MotionValue::from_keyframe(b);
                return MotionValue {
                    scale: start.scale + (end.scale - start.scale) * t,
                    pan_x: start.pan_x + (end.pan_x - start.pan_x) * t,
                    pan_y: start.pan_y + (end.pan_y - start.pan_y) * t,
                    rotation: start.rotation + (end.rotation - start.rotation) * t,
                    opacity: start.opacity + (end.opacity - start.opacity) * t,
                }
                .clamped();
            }
        }
        MotionValue::from_keyframe(last).clamped()
    }

    pub fn set_keyframe(&mut self, local_time: f64, value: MotionValue) {
        let local_time = if local_time.is_finite() {
            local_time.max(0.0)
        } else {
            0.0
        };
        let time = local_time + self.origin;
        let value = value.clamped();
        if let Some(existing) = self
            .keyframes
            .iter_mut()
            .find(|keyframe| (keyframe.time - time).abs() <= EPSILON)
        {
            existing.scale = value.scale;
            existing.pan_x = value.pan_x;
            existing.pan_y = value.pan_y;
            existing.rotation = value.rotation;
            existing.opacity = value.opacity;
        } else {
            self.keyframes.push(MotionKeyframe {
                time,
                scale: value.scale,
                pan_x: value.pan_x,
                pan_y: value.pan_y,
                rotation: value.rotation,
                opacity: value.opacity,
            });
        }
        self.sort_and_dedupe();
    }

    pub fn sort_and_dedupe(&mut self) {
        self.keyframes
            .retain(|keyframe| keyframe.time.is_finite() && keyframe.time >= 0.0);
        self.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
        let mut deduped: Vec<MotionKeyframe> = Vec::with_capacity(self.keyframes.len());
        for keyframe in self.keyframes.drain(..) {
            if deduped
                .last()
                .map(|last| (keyframe.time - last.time).abs() <= EPSILON)
                .unwrap_or(false)
            {
                if let Some(last) = deduped.last_mut() {
                    *last = keyframe;
                }
            } else {
                deduped.push(keyframe);
            }
        }
        self.keyframes = deduped;
    }

    /// Splits the clip at local `at` without changing the curve. The right side
    /// advances its origin so both pieces sample the original motion exactly.
    pub fn split(&self, at: f64) -> (ClipMotion, ClipMotion) {
        if self.is_empty() {
            return (ClipMotion::default(), ClipMotion::default());
        }
        let mut right = self.clone();
        right.origin += at.max(0.0);
        (self.clone(), right)
    }

    /// Advances or rewinds the curve origin when the clip's in-point changes.
    pub fn trim_start(&self, delta: f64, _new_duration: f64) -> ClipMotion {
        if self.is_empty() {
            return ClipMotion::default();
        }
        let mut result = self.clone();
        result.origin = (result.origin + delta).max(0.0);
        result
    }

    /// Trimming the end does not need to rewrite or discard curve data. Keeping
    /// later keyframes also lets the user extend the clip again without losing
    /// the original animation.
    pub fn trim_end(&self, _new_duration: f64) -> ClipMotion {
        if self.is_empty() {
            return ClipMotion::default();
        }
        self.clone()
    }

    fn motion_points(&self, duration: f64) -> Vec<(f64, MotionValue)> {
        let duration = duration.max(0.01);
        let mut points: Vec<(f64, MotionValue)> = Vec::new();
        points.push((0.0, self.sample(0.0)));
        for keyframe in self.sorted_keyframes() {
            let local_time = keyframe.time - self.origin;
            if local_time > EPSILON && local_time < duration - EPSILON {
                points.push((local_time, MotionValue::from_keyframe(keyframe).clamped()));
            }
        }
        points.push((duration, self.sample(duration)));
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        points.dedup_by(|a, b| (a.0 - b.0).abs() <= EPSILON);
        points
    }

    /// Returns the `zoompan` expressions for scale, horizontal pan, and
    /// vertical pan. `None` means the clip should not receive a zoom pass.
    pub fn zoompan_expressions(&self, duration: f64) -> Option<(String, String, String)> {
        if self.is_empty() {
            return None;
        }
        let duration = duration.max(0.01);
        let points = self.motion_points(duration);
        let frames: Vec<(i64, MotionValue)> = points
            .iter()
            .map(|(time, value)| ((time.clamp(0.0, duration) * FPS).round() as i64, *value))
            .collect();
        if frames.is_empty()
            || frames
                .iter()
                .all(|(_, value)| value.has_scale_pan_identity())
        {
            return None;
        }

        Some((
            piecewise(&frames, |value| value.scale, self.easing, 1.0, "on"),
            format!(
                "iw/2-(iw/zoom/2)+({})*iw*0.12",
                piecewise(&frames, |value| value.pan_x, self.easing, 0.0, "on")
            ),
            format!(
                "ih/2-(ih/zoom/2)+({})*ih*0.12",
                piecewise(&frames, |value| value.pan_y, self.easing, 0.0, "on")
            ),
        ))
    }

    /// Returns a radians expression for ffmpeg's `rotate` filter.
    pub fn rotation_expression(&self, duration: f64) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let points = self.motion_points(duration.max(0.01));
        if points
            .iter()
            .all(|(_, value)| value.rotation.abs() < EPSILON)
        {
            return None;
        }
        Some(piecewise_seconds(
            &points,
            |value| value.rotation.to_radians(),
            self.easing,
            0.0,
            "t",
        ))
    }

    /// Returns a 0..1 expression for the alpha channel in an RGBA pass.
    pub fn opacity_expression(&self, duration: f64) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let points = self.motion_points(duration.max(0.01));
        if points
            .iter()
            .all(|(_, value)| (value.opacity - 1.0).abs() < EPSILON)
        {
            return None;
        }
        Some(piecewise_seconds(
            &points,
            |value| value.opacity,
            self.easing,
            1.0,
            "T",
        ))
    }
}

fn ease(t: f64, easing: MotionEasing) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        MotionEasing::Linear => t,
        MotionEasing::EaseIn => t * t,
        MotionEasing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        MotionEasing::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
    }
}

fn eased_progress(u: &str, easing: MotionEasing) -> String {
    match easing {
        MotionEasing::Linear => u.to_string(),
        MotionEasing::EaseIn => format!("pow({u},2)"),
        MotionEasing::EaseOut => format!("(1-pow(1-{u},2))"),
        MotionEasing::EaseInOut => {
            format!("if(lt({u},0.5),2*pow({u},2),1-pow(-2*{u}+2,2)/2)")
        }
    }
}

fn piecewise(
    frames: &[(i64, MotionValue)],
    property: impl Fn(&MotionValue) -> f64,
    easing: MotionEasing,
    fallback: f64,
    variable: &str,
) -> String {
    if frames.is_empty() {
        return format!("{fallback:.6}");
    }
    if frames.len() == 1 {
        return format!("{:.6}", property(&frames[0].1));
    }
    let last = frames.last().expect("frames checked");
    let mut expression = format!("{:.6}", property(&last.1));
    for pair in frames.windows(2).rev() {
        let (start_frame, start_value) = pair[0];
        let (end_frame, end_value) = pair[1];
        let span = end_frame - start_frame;
        if span <= 0 {
            expression = format!("{:.6}", property(&end_value));
            continue;
        }
        let start = property(&start_value);
        let end = property(&end_value);
        let u = format!("(({variable}-{start_frame})/{span})");
        let eased = eased_progress(&u, easing);
        let segment = format!("{start:.6}+({end:.6}-{start:.6})*{eased}");
        expression = format!("if(lt({variable},{end_frame}),{segment},{expression})");
    }
    expression
}

fn piecewise_seconds(
    points: &[(f64, MotionValue)],
    property: impl Fn(&MotionValue) -> f64,
    easing: MotionEasing,
    fallback: f64,
    variable: &str,
) -> String {
    if points.is_empty() {
        return format!("{fallback:.6}");
    }
    if points.len() == 1 {
        return format!("{:.6}", property(&points[0].1));
    }
    let last = points.last().expect("points checked");
    let mut expression = format!("{:.6}", property(&last.1));
    for pair in points.windows(2).rev() {
        let (start_time, start_value) = pair[0];
        let (end_time, end_value) = pair[1];
        let span = end_time - start_time;
        if span <= 0.0 {
            expression = format!("{:.6}", property(&end_value));
            continue;
        }
        let start = property(&start_value);
        let end = property(&end_value);
        let u = format!("(({variable}-{start_time:.6})/{span:.6})");
        let eased = eased_progress(&u, easing);
        let segment = format!("{start:.6}+({end:.6}-{start:.6})*{eased}");
        expression = format!("if(lt({variable},{end_time:.6}),{segment},{expression})");
    }
    expression
}

#[cfg(test)]
mod tests {
    use super::*;

    fn motion() -> ClipMotion {
        ClipMotion {
            keyframes: vec![
                MotionKeyframe {
                    time: 0.0,
                    scale: 1.0,
                    pan_x: 0.0,
                    pan_y: 0.0,
                    rotation: 0.0,
                    opacity: 1.0,
                },
                MotionKeyframe {
                    time: 2.0,
                    scale: 2.0,
                    pan_x: 1.0,
                    pan_y: -1.0,
                    rotation: 90.0,
                    opacity: 0.0,
                },
            ],
            easing: MotionEasing::EaseInOut,
            origin: 0.0,
        }
    }

    fn assert_motion_eq(a: MotionValue, b: MotionValue) {
        assert!((a.scale - b.scale).abs() < 1e-6, "{a:?} != {b:?}");
        assert!((a.pan_x - b.pan_x).abs() < 1e-6, "{a:?} != {b:?}");
        assert!((a.pan_y - b.pan_y).abs() < 1e-6, "{a:?} != {b:?}");
        assert!((a.rotation - b.rotation).abs() < 1e-6, "{a:?} != {b:?}");
        assert!((a.opacity - b.opacity).abs() < 1e-6, "{a:?} != {b:?}");
    }

    #[test]
    fn samples_between_keyframes() {
        let value = motion().sample(1.0);
        assert!((value.scale - 1.5).abs() < 1e-6);
        assert!((value.pan_x - 0.5).abs() < 1e-6);
        assert!((value.pan_y + 0.5).abs() < 1e-6);
        assert!((value.rotation - 45.0).abs() < 1e-6);
        assert!((value.opacity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn split_preserves_the_sampled_curve() {
        let original = motion();
        let at = 0.9;
        let (left, right) = original.split(at);
        for time in [0.0, 0.2, 0.5, 0.89, 0.9, 1.2, 1.7, 1.99] {
            let expected = original.sample(time);
            let actual = if time <= at {
                left.sample(time)
            } else {
                right.sample(time - at)
            };
            assert_motion_eq(expected, actual);
        }
    }

    #[test]
    fn trim_start_preserves_the_sampled_curve() {
        let original = motion();
        let delta = 0.55;
        let trimmed = original.trim_start(delta, 1.45);
        for local in [0.0, 0.2, 0.7, 1.0, 1.4] {
            assert_motion_eq(original.sample(local + delta), trimmed.sample(local));
        }
    }

    #[test]
    fn trim_end_preserves_the_sampled_curve() {
        let original = motion();
        let trimmed = original.trim_end(1.2);
        for local in [0.0, 0.2, 0.7, 1.0, 1.19] {
            assert_motion_eq(original.sample(local), trimmed.sample(local));
        }
    }

    #[test]
    fn expressions_are_generated_for_each_transform() {
        let motion = motion();
        let (scale, pan_x, pan_y) = motion.zoompan_expressions(2.0).expect("scale/pan motion");
        assert!(scale.contains("if(lt(on,"));
        assert!(pan_x.contains("iw/zoom"));
        assert!(pan_y.contains("ih/zoom"));
        let rotation = motion.rotation_expression(2.0).expect("rotation motion");
        assert!(rotation.contains("if(lt(t,"));
        let opacity = motion.opacity_expression(2.0).expect("opacity motion");
        assert!(opacity.contains("if(lt(T,"));
    }

    #[test]
    fn identity_motion_has_no_expression() {
        let identity = ClipMotion {
            keyframes: vec![MotionKeyframe {
                time: 0.0,
                scale: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
                rotation: 0.0,
                opacity: 1.0,
            }],
            easing: MotionEasing::Linear,
            origin: 0.0,
        };
        assert!(identity.zoompan_expressions(2.0).is_none());
        assert!(identity.rotation_expression(2.0).is_none());
        assert!(identity.opacity_expression(2.0).is_none());
    }
}
