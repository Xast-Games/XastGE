use glam::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Ease {
    Linear,
    #[default]
    OutCubic,
    InOutCubic,
    OutBack,
}

impl Ease {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::OutCubic => 1.0 - (1.0 - t).powi(3),
            Ease::InOutCubic => {
                if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
            }
            Ease::OutBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
            }
        }
    }
}

/// Hover/press visual changes ease over `duration` seconds, and reverse smoothly midway.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub duration: f32,
    pub ease: Ease,
}

/// Plays once when a node is created: fades in from `opacity` and slides from `offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enter {
    pub opacity: f32,
    pub offset: Vec2,
    pub duration: f32,
    pub delay: f32,
    pub ease: Ease,
}

impl Enter {
    pub fn fade() -> Enter {
        Enter {
            opacity: 0.0,
            offset: Vec2::ZERO,
            duration: 0.3,
            delay: 0.0,
            ease: Ease::OutCubic,
        }
    }

    pub fn slide(mut self, offset: Vec2) -> Enter {
        self.offset = offset;
        self
    }

    pub fn duration(mut self, seconds: f32) -> Enter {
        self.duration = seconds;
        self
    }

    pub fn delay(mut self, seconds: f32) -> Enter {
        self.delay = seconds;
        self
    }

    pub fn ease(mut self, ease: Ease) -> Enter {
        self.ease = ease;
        self
    }
}

/// Moves `value` toward `target`, covering 0..1 in `duration` seconds.
pub(crate) fn approach(value: f32, target: f32, dt: f32, duration: f32) -> f32 {
    if duration <= 0.0 {
        return target;
    }

    let step = dt / duration;
    if value < target { (value + step).min(target) } else { (value - step).max(target) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_hits_endpoints() {
        for ease in [Ease::Linear, Ease::OutCubic, Ease::InOutCubic, Ease::OutBack] {
            assert!(ease.apply(0.0).abs() < 1e-5, "{ease:?} at 0");
            assert!((ease.apply(1.0) - 1.0).abs() < 1e-5, "{ease:?} at 1");
        }
    }

    #[test]
    fn approach_reverses_without_jumping() {
        let up = approach(0.0, 1.0, 0.05, 0.1);
        assert_eq!(up, 0.5);

        let back = approach(up, 0.0, 0.05, 0.1);
        assert_eq!(back, 0.0);

        assert_eq!(approach(0.3, 1.0, 0.01, 0.0), 1.0);
    }
}
