use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use glam::Vec2;
use noise::{Fbm, Perlin};
use parking_lot::Mutex;
use rayon::prelude::*;

use super::source::{HeightSource, NoiseSource, SurfaceHints};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseRegion {
    pub origin: Vec2,
    pub spacing: f32,
    pub size: usize,
}

impl BaseRegion {
    pub fn covering(center: Vec2, extent: f32, spacing: f32) -> BaseRegion {
        let spacing = spacing.max(0.5);
        let size = ((extent / spacing).ceil() as usize + 1).clamp(16, 2049);
        let half = (size - 1) as f32 * spacing * 0.5;
        BaseRegion { origin: center - Vec2::splat(half), spacing, size }
    }
}

#[derive(Debug, Clone)]
pub struct BaseMap {
    pub region: BaseRegion,
    pub heights: Vec<f32>,
    pub moisture: Vec<f32>,
    pub sediment: Vec<f32>,
    pub detail: Vec<f32>,
}

fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
}

impl BaseMap {
    pub fn generate(region: BaseRegion, source: &dyn HeightSource) -> BaseMap {
        let n = region.size;
        let mut heights = vec![0.0; n * n];
        heights.par_chunks_mut(n).enumerate().for_each(|(j, row)| {
            let z = region.origin.y as f64 + j as f64 * region.spacing as f64;
            for (i, h) in row.iter_mut().enumerate() {
                let x = region.origin.x as f64 + i as f64 * region.spacing as f64;
                *h = source.height(x, z);
            }
        });
        BaseMap { region, moisture: vec![0.0; n * n], sediment: vec![0.0; n * n], detail: vec![1.0; n * n], heights }
    }

    pub fn sample_layer(&self, layer: &[f32], x: f64, z: f64) -> f32 {
        let r = &self.region;
        let gx = ((x - r.origin.x as f64) / r.spacing as f64).clamp(0.0, (r.size - 1) as f64);
        let gz = ((z - r.origin.y as f64) / r.spacing as f64).clamp(0.0, (r.size - 1) as f64);
        let (i, j) = ((gx.floor() as usize).min(r.size - 2), (gz.floor() as usize).min(r.size - 2));
        let (tx, tz) = ((gx - i as f64) as f32, (gz - j as f64) as f32);
        let at = |i: usize, j: usize| layer[j * r.size + i];

        let top = at(i, j) + (at(i + 1, j) - at(i, j)) * tx;
        let bottom = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * tx;
        top + (bottom - top) * tz
    }

    fn at(&self, i: isize, j: isize) -> f32 {
        let last = self.region.size as isize - 1;
        let (i, j) = (i.clamp(0, last) as usize, j.clamp(0, last) as usize);
        self.heights[j * self.region.size + i]
    }

    pub fn sample(&self, x: f64, z: f64) -> f32 {
        let r = &self.region;
        let gx = ((x - r.origin.x as f64) / r.spacing as f64).clamp(0.0, (r.size - 1) as f64);
        let gz = ((z - r.origin.y as f64) / r.spacing as f64).clamp(0.0, (r.size - 1) as f64);
        let (i, j) = (gx.floor() as isize, gz.floor() as isize);
        let (tx, tz) = ((gx - i as f64) as f32, (gz - j as f64) as f32);

        let row = |dj: isize| {
            catmull_rom(
                self.at(i - 1, j + dj),
                self.at(i, j + dj),
                self.at(i + 1, j + dj),
                self.at(i + 2, j + dj),
                tx,
            )
        };
        catmull_rom(row(-1), row(0), row(1), row(2), tz)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErosionSettings {
    pub droplets_per_cell: f32,
    pub lifetime: u32,
    pub inertia: f32,
    pub capacity: f32,
    pub min_capacity: f32,
    pub erode_speed: f32,
    pub deposit_speed: f32,
    pub evaporation: f32,
    pub gravity: f32,
    pub radius: u32,
    pub thermal_iterations: u32,
    pub talus_angle: f32,
    pub thermal_rate: f32,
}

impl Default for ErosionSettings {
    fn default() -> Self {
        ErosionSettings {
            droplets_per_cell: 1.0,
            lifetime: 40,
            inertia: 0.05,
            capacity: 4.0,
            min_capacity: 0.01,
            erode_speed: 0.3,
            deposit_speed: 0.3,
            evaporation: 0.01,
            gravity: 4.0,
            radius: 3,
            thermal_iterations: 30,
            talus_angle: 38.0,
            thermal_rate: 0.3,
        }
    }
}

struct Brush {
    offsets: Vec<(isize, isize, f32)>,
}

impl Brush {
    fn new(radius: u32) -> Brush {
        let r = radius.max(1) as isize;
        let mut offsets = Vec::new();
        for dy in -r..=r {
            for dx in -r..=r {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                if d <= r as f32 {
                    offsets.push((dx, dy, r as f32 - d + 1e-3));
                }
            }
        }
        let total: f32 = offsets.iter().map(|o| o.2).sum();
        for o in &mut offsets {
            o.2 /= total;
        }
        Brush { offsets }
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn height_and_gradient(map: &[f32], size: usize, x: f32, y: f32) -> (f32, Vec2) {
    let (cx, cy) = (x as usize, y as usize);
    let (u, v) = (x - cx as f32, y - cy as f32);
    let i = cy * size + cx;
    let (nw, ne, sw, se) = (map[i], map[i + 1], map[i + size], map[i + size + 1]);

    let gradient = Vec2::new((ne - nw) * (1.0 - v) + (se - sw) * v, (sw - nw) * (1.0 - u) + (se - ne) * u);
    let height = nw * (1.0 - u) * (1.0 - v) + ne * u * (1.0 - v) + sw * (1.0 - u) * v + se * u * v;
    (height, gradient)
}

pub struct ErosionTrace<'a> {
    pub flow: &'a mut [f32],
    pub deposit: &'a mut [f32],
}

pub fn erode_hydraulic(
    map: &mut [f32],
    size: usize,
    settings: &ErosionSettings,
    seed: u64,
    trace: ErosionTrace,
    mut keep_going: impl FnMut(f32) -> bool,
) -> bool {
    let brush = Brush::new(settings.radius);
    let droplets = ((size * size) as f32 * settings.droplets_per_cell.max(0.0)) as usize;
    let limit = (size - 1) as f32;
    let mut rng = Rng(seed);

    for d in 0..droplets {
        if d % 4096 == 0 && !keep_going(d as f32 / droplets.max(1) as f32) {
            return false;
        }

        let mut pos = Vec2::new(rng.unit() * (limit - 1.0), rng.unit() * (limit - 1.0));
        let mut dir = Vec2::ZERO;
        let (mut speed, mut water, mut sediment) = (1.0f32, 1.0f32, 0.0f32);

        for _ in 0..settings.lifetime {
            let (cx, cy) = (pos.x as usize, pos.y as usize);
            let (u, v) = (pos.x - cx as f32, pos.y - cy as f32);
            let (height, gradient) = height_and_gradient(map, size, pos.x, pos.y);
            trace.flow[cy * size + cx] += water;

            dir = dir * settings.inertia - gradient * (1.0 - settings.inertia);
            let len = dir.length();
            if len < 1e-9 {
                break;
            }
            dir /= len;
            pos += dir;

            if pos.x < 0.0 || pos.y < 0.0 || pos.x >= limit || pos.y >= limit {
                break;
            }

            let (new_height, _) = height_and_gradient(map, size, pos.x, pos.y);
            let dh = new_height - height;
            let capacity = (-dh * speed * water * settings.capacity).max(settings.min_capacity);

            if sediment > capacity || dh > 0.0 {
                let amount = if dh > 0.0 { dh.min(sediment) } else { (sediment - capacity) * settings.deposit_speed };
                sediment -= amount;

                let i = cy * size + cx;
                trace.deposit[i] += amount;
                map[i] += amount * (1.0 - u) * (1.0 - v);
                map[i + 1] += amount * u * (1.0 - v);
                map[i + size] += amount * (1.0 - u) * v;
                map[i + size + 1] += amount * u * v;
            } else {
                let amount = ((capacity - sediment) * settings.erode_speed).min(-dh);
                for &(dx, dy, w) in &brush.offsets {
                    let (x, y) = (cx as isize + dx, cy as isize + dy);
                    if x < 0 || y < 0 || x >= size as isize || y >= size as isize {
                        continue;
                    }
                    let k = y as usize * size + x as usize;
                    let take = (amount * w).min(map[k].max(0.0));
                    map[k] -= take;
                    sediment += take;
                }
            }

            speed = (speed * speed - dh * settings.gravity).max(0.0).sqrt();
            water *= 1.0 - settings.evaporation;
        }
    }

    true
}

pub fn erode_thermal(map: &mut Vec<f32>, size: usize, iterations: u32, talus: f32, rate: f32) {
    let flow = |a: f32, b: f32| (a - b - talus).max(0.0) * rate * 0.25;
    let neighbours: [(isize, isize); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

    for _ in 0..iterations {
        let current = map.clone();
        map.par_chunks_mut(size).enumerate().for_each(|(y, row)| {
            for (x, h) in row.iter_mut().enumerate() {
                let here = current[y * size + x];
                let mut delta = 0.0;
                for (dx, dy) in neighbours {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx < 0 || ny < 0 || nx >= size as isize || ny >= size as isize {
                        continue;
                    }
                    let there = current[ny as usize * size + nx as usize];
                    delta += flow(there, here) - flow(here, there);
                }
                *h = here + delta;
            }
        });
    }
}

pub fn build_base(
    region: BaseRegion,
    source: &dyn HeightSource,
    settings: &ErosionSettings,
    seed: u64,
    keep_going: impl FnMut(f32) -> bool,
) -> Option<BaseMap> {
    let mut base = BaseMap::generate(region, source);
    let n = region.size;

    let (min, max) = base.heights.iter().fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(*h), hi.max(*h)));
    let range = (max - min).max(1e-3);
    for h in &mut base.heights {
        *h = (*h - min) / range;
    }

    let mut flow = vec![0.0; n * n];
    let mut deposit = vec![0.0; n * n];
    let trace = ErosionTrace { flow: &mut flow, deposit: &mut deposit };
    if !erode_hydraulic(&mut base.heights, n, settings, seed, trace, keep_going) {
        return None;
    }

    base.moisture = normalize_trace(&flow, n);
    base.sediment = normalize_trace(&deposit, n);

    let talus = settings.talus_angle.to_radians().tan() * region.spacing / range;
    erode_thermal(&mut base.heights, n, settings.thermal_iterations, talus, settings.thermal_rate.clamp(0.0, 1.0));

    for h in &mut base.heights {
        *h = *h * range + min;
    }

    let spacing = region.spacing as f64;
    let origin = (region.origin.x as f64, region.origin.y as f64);
    base.heights
        .par_chunks_mut(n)
        .zip(base.detail.par_chunks_mut(n))
        .enumerate()
        .for_each(|(j, (heights, detail))| {
            let z = origin.1 + j as f64 * spacing;
            for (i, (h, d)) in heights.iter_mut().zip(detail.iter_mut()).enumerate() {
                let x = origin.0 + i as f64 * spacing;
                *h = source.pin(x, z, *h);
                *d = source.detail_weight(x, z);
            }
        });

    Some(base)
}

fn normalize_trace(values: &[f32], size: usize) -> Vec<f32> {
    let mut blurred = vec![0.0; values.len()];
    for y in 0..size {
        for x in 0..size {
            let mut sum = 0.0;
            let mut count = 0.0;
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx >= 0 && ny >= 0 && (nx as usize) < size && (ny as usize) < size {
                        sum += values[ny as usize * size + nx as usize];
                        count += 1.0;
                    }
                }
            }
            blurred[y * size + x] = sum / count;
        }
    }

    let mut sorted: Vec<f32> = blurred.iter().copied().filter(|v| *v > 0.0).collect();
    if sorted.is_empty() {
        return blurred;
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    let reference = sorted[(sorted.len() as f32 * 0.95) as usize].max(1e-6);

    blurred.iter().map(|v| (v.ln_1p() / reference.ln_1p()).clamp(0.0, 1.0)).collect()
}

pub struct ErodedSource {
    pub base: Arc<BaseMap>,
    pub detail: NoiseSource<Fbm<Perlin>>,
}

impl ErodedSource {
    pub fn new(base: Arc<BaseMap>, seed: u64, detail_amplitude: f32, detail_wavelength: f32) -> ErodedSource {
        ErodedSource {
            base,
            detail: NoiseSource::fbm(seed ^ 0xDE7A11, 0.0, detail_amplitude, detail_wavelength as f64, 4),
        }
    }
}

impl HeightSource for ErodedSource {
    fn height(&self, x: f64, z: f64) -> f32 {
        let weight = self.base.sample_layer(&self.base.detail, x, z);
        self.base.sample(x, z) + self.detail.height(x, z) * weight
    }

    fn surface(&self, x: f64, z: f64) -> SurfaceHints {
        SurfaceHints {
            moisture: self.base.sample_layer(&self.base.moisture, x, z),
            sediment: self.base.sample_layer(&self.base.sediment, x, z),
        }
    }
}

pub struct BaseJob {
    generation: Arc<AtomicU64>,
    progress: Arc<AtomicU32>,
    result: Arc<Mutex<Option<(u64, Arc<BaseMap>)>>>,
    current: u64,
    started: Option<Instant>,
    finished: Option<Duration>,
}

impl Default for BaseJob {
    fn default() -> Self {
        BaseJob {
            generation: Arc::new(AtomicU64::new(0)),
            progress: Arc::new(AtomicU32::new(0)),
            result: Arc::new(Mutex::new(None)),
            current: 0,
            started: None,
            finished: None,
        }
    }
}

impl BaseJob {
    pub fn start(
        &mut self,
        region: BaseRegion,
        source: Arc<dyn HeightSource>,
        settings: ErosionSettings,
        seed: u64,
    ) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.current = generation;
        self.started = Some(Instant::now());
        self.finished = None;
        self.progress.store(0, Ordering::Relaxed);

        let current = self.generation.clone();
        let progress = self.progress.clone();
        let result = self.result.clone();

        std::thread::spawn(move || {
            let keep_going = |fraction: f32| {
                progress.store((fraction * 1000.0) as u32, Ordering::Relaxed);
                current.load(Ordering::Relaxed) == generation
            };

            if let Some(base) = build_base(region, source.as_ref(), &settings, seed, keep_going)
                && current.load(Ordering::Relaxed) == generation
            {
                progress.store(1000, Ordering::Relaxed);
                *result.lock() = Some((generation, Arc::new(base)));
            }
        });
    }

    pub fn poll(&mut self) -> Option<Arc<BaseMap>> {
        let mut slot = self.result.lock();
        match slot.take() {
            Some((generation, base)) if generation == self.current => {
                self.finished = self.started.map(|s| s.elapsed());
                Some(base)
            }
            _ => None,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.started.is_some() && self.finished.is_none()
    }

    pub fn progress(&self) -> f32 {
        self.progress.load(Ordering::Relaxed) as f32 / 1000.0
    }

    pub fn since_start(&self) -> Option<Duration> {
        self.started.map(|s| s.elapsed())
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.finished.or_else(|| self.since_start())
    }
}
