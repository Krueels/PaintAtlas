//! Stroke filtering and pressure response, independent from interface widgets.
pub const MAX_BRUSH_SIZE: f32 = 1000.0;
#[derive(Clone, Copy)]
pub struct Stabilizer {
    point: [f32; 2],
    stage: [f32; 2],
}
impl Stabilizer {
    pub fn new(point: [f32; 2]) -> Self {
        Self {
            point,
            stage: point,
        }
    }
    /// Two cascaded low-pass stages integrated exactly for elapsed seconds.
    /// The strength does not disappear at speed or change with frame rate/zoom.
    pub fn feed(&mut self, raw: [f32; 2], amount: f32, dt: f32) -> [f32; 2] {
        if amount <= 0.0 {
            self.point = raw;
            self.stage = raw;
            return raw;
        }
        let tau = 0.001 + 0.119 * (amount.clamp(0.0, 100.0) / 100.0).powf(1.5);
        let t = dt.clamp(0.0, 0.25) / tau;
        let decay = (-t).exp();
        for (axis, target) in raw.into_iter().enumerate() {
            let first = self.stage[axis] - target;
            let second = self.point[axis] - target;
            self.stage[axis] = target + first * decay;
            self.point[axis] = target + (second + first * t) * decay;
        }
        self.point
    }
    /// Drain the filter along its curved continuation instead of bypassing it
    /// with one long straight segment when the pen is lifted.
    pub fn finish(&mut self, target: [f32; 2], amount: f32, min_distance: f32) -> Vec<[f32; 2]> {
        if amount <= 0.0 {
            return vec![self.feed(target, 0.0, 0.0)];
        }
        let mut points = Vec::new();
        let mut last = self.point;
        for _ in 0..256 {
            let p = self.feed(target, amount, 1.0 / 120.0);
            let distance = (p[0] - last[0]).hypot(p[1] - last[1]);
            if distance >= min_distance.max(0.5) {
                points.push(p);
                last = p;
            }
            if (p[0] - target[0]).hypot(p[1] - target[1]) < 0.01
                && (self.stage[0] - target[0]).hypot(self.stage[1] - target[1]) < 0.01
            {
                break;
            }
        }
        if points.last() != Some(&target) {
            points.push(target);
        }
        self.point = target;
        self.stage = target;
        points
    }
}
pub fn pressure_size(pressure: f32, gamma: f32, minimum: f32) -> f32 {
    minimum + (1.0 - minimum) * pressure.clamp(0.0, 1.0).powf(gamma)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_filter_is_exact_and_filter_reduces_jitter() {
        assert_eq!(
            Stabilizer::new([0.0, 0.0]).feed([8.0, 3.0], 0.0, 1.0 / 120.0),
            [8.0, 3.0]
        );
        let mut filter = Stabilizer::new([0.0, 0.0]);
        let mut amplitude = 0.0_f32;
        for i in 0..100 {
            let p = filter.feed(
                [0.0, if i % 2 == 0 { 1.0 } else { -1.0 }],
                60.0,
                1.0 / 120.0,
            );
            amplitude = amplitude.max(p[1].abs());
        }
        assert!(amplitude < 0.15);
    }
    #[test]
    fn stabilization_strength_remains_effective_at_high_speed() {
        let mut previous = 1001.0;
        for amount in [0.0, 25.0, 50.0, 100.0] {
            let mut filter = Stabilizer::new([0.0, 0.0]);
            let p = filter.feed([1000.0, 0.0], amount, 1.0 / 60.0);
            assert!(
                p[0] < previous,
                "increasing strength must increase smoothing"
            );
            previous = p[0];
        }
        assert!(
            previous > 0.0 && previous < 20.0,
            "100% must not snap to a fast raw sample"
        );
    }
    #[test]
    fn smoothing_is_independent_of_frame_rate_and_coordinate_scale() {
        let mut reference: Option<[f32; 2]> = None;
        for fps in [30, 60, 120, 240] {
            for scale in [0.125, 1.0, 8.0] {
                let mut filter = Stabilizer::new([0.0, 0.0]);
                let mut p = [0.0; 2];
                for _ in 0..fps {
                    p = filter.feed([1000.0 * scale, 200.0 * scale], 100.0, 1.0 / fps as f32);
                }
                let normalized = [p[0] / scale, p[1] / scale];
                if let Some([x, y]) = reference {
                    assert!((normalized[0] - x).abs() < 0.02);
                    assert!((normalized[1] - y).abs() < 0.02);
                } else {
                    reference = Some(normalized);
                }
            }
        }
    }
    #[test]
    fn release_finishes_at_the_endpoint_without_a_straight_shortcut() {
        let mut filter = Stabilizer::new([0.0, 0.0]);
        filter.feed([100.0, 0.0], 100.0, 0.1);
        let start = filter.feed([100.0, 100.0], 100.0, 0.02);
        let points = filter.finish([100.0, 100.0], 100.0, 0.5);
        assert_eq!(points.last(), Some(&[100.0, 100.0]));
        assert!(points.len() > 10);
        let end = [100.0 - start[0], 100.0 - start[1]];
        assert!(
            points
                .iter()
                .any(|p| ((p[0] - start[0]) * end[1] - (p[1] - start[1]) * end[0]).abs() > 10.0),
            "tail must follow the filter's curve"
        );
    }
    #[test]
    fn pressure_curves_are_monotonic_and_keep_endpoints() {
        for gamma in [0.3, 1.0, 3.0] {
            assert_eq!(pressure_size(0.0, gamma, 0.05), 0.05);
            assert_eq!(pressure_size(1.0, gamma, 0.05), 1.0);
            assert!(pressure_size(0.2, gamma, 0.05) < pressure_size(0.8, gamma, 0.05));
        }
        assert!(pressure_size(0.25, 0.5, 0.05) > pressure_size(0.25, 2.0, 0.05));
    }
}
