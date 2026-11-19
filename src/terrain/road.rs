use glam::{Vec2, Vec3};

const SUBDIVISIONS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadFrame {
    pub s: f32,
    pub position: Vec3,
    pub tangent: Vec3,
    pub right: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClosestPoint {
    pub s: f32,
    pub distance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ArcSample {
    s: f32,
    param: f32,
}

#[derive(Debug, Clone)]
pub struct RoadSpline {
    points: Vec<Vec3>,
    table: Vec<ArcSample>,
}

fn centripetal(p: [Vec3; 4], u: f32) -> Vec3 {
    let knot = |a: Vec3, b: Vec3| a.distance(b).sqrt().max(1e-4);
    let t0 = 0.0;
    let t1 = t0 + knot(p[0], p[1]);
    let t2 = t1 + knot(p[1], p[2]);
    let t3 = t2 + knot(p[2], p[3]);
    let t = t1 + (t2 - t1) * u;

    let a1 = p[0] * ((t1 - t) / (t1 - t0)) + p[1] * ((t - t0) / (t1 - t0));
    let a2 = p[1] * ((t2 - t) / (t2 - t1)) + p[2] * ((t - t1) / (t2 - t1));
    let a3 = p[2] * ((t3 - t) / (t3 - t2)) + p[3] * ((t - t2) / (t3 - t2));
    let b1 = a1 * ((t2 - t) / (t2 - t0)) + a2 * ((t - t0) / (t2 - t0));
    let b2 = a2 * ((t3 - t) / (t3 - t1)) + a3 * ((t - t1) / (t3 - t1));
    b1 * ((t2 - t) / (t2 - t1)) + b2 * ((t - t1) / (t2 - t1))
}

impl RoadSpline {
    pub fn new(points: impl IntoIterator<Item = Vec3>) -> Option<RoadSpline> {
        let mut deduped: Vec<Vec3> = Vec::new();
        for p in points {
            if deduped.last().is_none_or(|last| last.distance(p) > 1e-3) {
                deduped.push(p);
            }
        }
        if deduped.len() < 2 {
            return None;
        }

        let mut spline = RoadSpline { points: deduped, table: Vec::new() };
        spline.build_table();
        Some(spline)
    }

    fn segments(&self) -> usize {
        self.points.len() - 1
    }

    fn control(&self, i: isize) -> Vec3 {
        let n = self.points.len() as isize;
        if i < 0 {
            self.points[0] * 2.0 - self.points[1]
        } else if i >= n {
            self.points[n as usize - 1] * 2.0 - self.points[n as usize - 2]
        } else {
            self.points[i as usize]
        }
    }

    fn evaluate(&self, param: f32) -> Vec3 {
        let segment = (param.floor() as usize).min(self.segments() - 1);
        let u = (param - segment as f32).clamp(0.0, 1.0);
        let i = segment as isize;
        centripetal([self.control(i - 1), self.control(i), self.control(i + 1), self.control(i + 2)], u)
    }

    fn build_table(&mut self) {
        let steps = self.segments() * SUBDIVISIONS;
        let mut table = Vec::with_capacity(steps + 1);
        let mut s = 0.0;
        let mut previous = self.evaluate(0.0);
        table.push(ArcSample { s: 0.0, param: 0.0 });

        for k in 1..=steps {
            let param = k as f32 / SUBDIVISIONS as f32;
            let position = self.evaluate(param);
            s += previous.distance(position);
            table.push(ArcSample { s, param });
            previous = position;
        }

        self.table = table;
    }

    fn param_at(&self, s: f32) -> f32 {
        let s = s.clamp(0.0, self.length());
        let k = self.table.partition_point(|sample| sample.s < s).clamp(1, self.table.len() - 1);
        let (a, b) = (self.table[k - 1], self.table[k]);
        let t = if b.s > a.s { (s - a.s) / (b.s - a.s) } else { 0.0 };
        a.param + (b.param - a.param) * t
    }

    pub fn points(&self) -> &[Vec3] {
        &self.points
    }

    pub fn length(&self) -> f32 {
        self.table.last().map_or(0.0, |sample| sample.s)
    }

    pub fn control_s(&self, index: usize) -> f32 {
        let k = (index.min(self.segments()) * SUBDIVISIONS).min(self.table.len() - 1);
        self.table[k].s
    }

    pub fn position(&self, s: f32) -> Vec3 {
        self.evaluate(self.param_at(s))
    }

    pub fn frame(&self, s: f32) -> RoadFrame {
        let s = s.clamp(0.0, self.length());
        let h = 0.25f32.min(self.length() * 0.5);
        let a = self.position((s - h).max(0.0));
        let b = self.position((s + h).min(self.length()));
        let tangent = (b - a).normalize_or_zero();

        let flat = Vec3::new(tangent.x, 0.0, tangent.z).normalize_or_zero();
        let right = if flat == Vec3::ZERO { Vec3::X } else { flat.cross(Vec3::Y) };

        RoadFrame { s, position: self.position(s), tangent, right }
    }

    pub fn sample(&self, step: f32) -> Vec<RoadFrame> {
        let length = self.length();
        let count = ((length / step.max(1e-3)).ceil() as usize).max(1);
        (0..=count).map(|i| self.frame(length * i as f32 / count as f32)).collect()
    }

    pub fn closest(&self, point: Vec2) -> ClosestPoint {
        let mut best = ClosestPoint { s: 0.0, distance: f32::INFINITY };
        let mut best_abs = f32::INFINITY;

        let flat = |param: f32| {
            let p = self.evaluate(param);
            Vec2::new(p.x, p.z)
        };

        for pair in self.table.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            let (a, b) = (flat(start.param), flat(end.param));
            let ab = b - a;
            let len2 = ab.length_squared();
            let t = if len2 > 0.0 { ((point - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let projected = a + ab * t;
            let offset = point - projected;
            let abs = offset.length();

            if abs < best_abs {
                let side = Vec2::new(-ab.y, ab.x);
                let sign = if side.dot(offset) < 0.0 { -1.0 } else { 1.0 };
                best_abs = abs;
                best = ClosestPoint { s: start.s + (end.s - start.s) * t, distance: abs * sign };
            }
        }

        best
    }
}

#[derive(Debug, Clone)]
pub struct RoadProfile {
    pub step: f32,
    pub heights: Vec<f32>,
}

fn moving_average(values: &[f32], radius: usize) -> Vec<f32> {
    if radius == 0 {
        return values.to_vec();
    }

    let n = values.len();
    let mut prefix = Vec::with_capacity(n + 1);
    prefix.push(0.0f64);
    for v in values {
        prefix.push(prefix.last().unwrap() + *v as f64);
    }

    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(radius);
            let hi = (i + radius + 1).min(n);
            ((prefix[hi] - prefix[lo]) / (hi - lo) as f64) as f32
        })
        .collect()
}

fn limit_grade(values: &[f32], max_delta: f32) -> Vec<f32> {
    let n = values.len();
    let mut upper = values.to_vec();
    let mut lower = values.to_vec();

    for i in 1..n {
        upper[i] = upper[i].min(upper[i - 1] + max_delta);
        lower[i] = lower[i].max(lower[i - 1] - max_delta);
    }
    for i in (0..n.saturating_sub(1)).rev() {
        upper[i] = upper[i].min(upper[i + 1] + max_delta);
        lower[i] = lower[i].max(lower[i + 1] - max_delta);
    }

    upper.iter().zip(&lower).map(|(u, l)| (u + l) * 0.5).collect()
}

impl RoadProfile {
    pub fn build(
        spline: &RoadSpline,
        step: f32,
        smoothing: f32,
        max_grade: f32,
        ground: impl Fn(Vec2) -> f32,
    ) -> RoadProfile {
        let length = spline.length();
        let count = ((length / step.max(1e-3)).ceil() as usize).max(1) + 1;
        let step = length / (count - 1) as f32;

        let raw: Vec<f32> = (0..count)
            .map(|i| {
                let p = spline.position(i as f32 * step);
                ground(Vec2::new(p.x, p.z))
            })
            .collect();

        let radius = if step > 0.0 { (smoothing / step).round() as usize } else { 0 };
        let smoothed = moving_average(&raw, radius);
        let heights = limit_grade(&smoothed, max_grade.max(0.0) * step);

        RoadProfile { step, heights }
    }

    pub fn length(&self) -> f32 {
        self.step * (self.heights.len().saturating_sub(1)) as f32
    }

    pub fn height(&self, s: f32) -> f32 {
        let last = self.heights.len() - 1;
        if last == 0 || self.step <= 0.0 {
            return self.heights[0];
        }

        let x = (s / self.step).clamp(0.0, last as f32);
        let i = (x.floor() as usize).min(last - 1);
        let t = x - i as f32;
        self.heights[i] + (self.heights[i + 1] - self.heights[i]) * t
    }
}
