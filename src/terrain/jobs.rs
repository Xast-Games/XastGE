use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

use parking_lot::Mutex;

use crate::mesh::Mesh;

use super::{
    config::TerrainConfig,
    mesh::{TerrainVertex, tile_mesh},
    source::HeightSource,
    splat::SplatRules,
    tile::{TileCoord, TileData},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

pub struct TileBuild {
    pub coord: TileCoord,
    pub tile: TileData,
    pub mesh: Mesh<TerrainVertex>,
}

struct Finished {
    generation: u64,
    build: TileBuild,
}

pub struct TileJobs {
    sender: Sender<Finished>,
    receiver: Mutex<Receiver<Finished>>,
    generation: Arc<AtomicU64>,
    total: usize,
    received: usize,
    started: Option<Instant>,
    finished: Option<Duration>,
}

impl Default for TileJobs {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        TileJobs {
            sender,
            receiver: Mutex::new(receiver),
            generation: Arc::new(AtomicU64::new(0)),
            total: 0,
            received: 0,
            started: None,
            finished: None,
        }
    }
}

impl TileJobs {
    pub fn start(
        &mut self,
        config: &TerrainConfig,
        rules: &SplatRules,
        source: Arc<dyn HeightSource>,
        coords: impl IntoIterator<Item = TileCoord>,
    ) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let config = Arc::new(config.clone());
        let rules = Arc::new(rules.clone());

        self.total = 0;
        self.received = 0;
        self.started = Some(Instant::now());
        self.finished = None;

        for coord in coords {
            self.total += 1;

            let config = config.clone();
            let rules = rules.clone();
            let source = source.clone();
            let sender = self.sender.clone();
            let current = self.generation.clone();

            rayon::spawn(move || {
                if current.load(Ordering::Relaxed) != generation {
                    return;
                }
                let tile = TileData::generate_with(&config, coord, source.as_ref(), &rules);

                if current.load(Ordering::Relaxed) != generation {
                    return;
                }
                let mesh = tile_mesh(&config, &tile);

                let _ = sender.send(Finished { generation, build: TileBuild { coord, tile, mesh } });
            });
        }

        if self.total == 0 {
            self.finished = Some(Duration::ZERO);
        }
    }

    pub fn poll(&mut self, max: usize) -> Vec<TileBuild> {
        let current = self.generation.load(Ordering::Relaxed);
        let mut ready = Vec::new();

        {
            let receiver = self.receiver.lock();
            while ready.len() < max {
                match receiver.try_recv() {
                    Ok(done) if done.generation == current => ready.push(done.build),
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }

        self.received += ready.len();
        if !self.is_busy() && self.finished.is_none() {
            self.finished = self.started.map(|s| s.elapsed());
        }

        ready
    }

    pub fn progress(&self) -> Progress {
        Progress { done: self.received, total: self.total }
    }

    pub fn is_busy(&self) -> bool {
        self.received < self.total
    }

    pub fn since_start(&self) -> Option<Duration> {
        self.started.map(|s| s.elapsed())
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.finished.or_else(|| self.since_start())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::terrain::source::FlatSource;

    fn config() -> TerrainConfig {
        TerrainConfig { cells: 16, tile_size: 64.0, lod_levels: 3, ..Default::default() }
    }

    fn drain(jobs: &mut TileJobs) -> Vec<TileBuild> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut all = Vec::new();
        while jobs.is_busy() && Instant::now() < deadline {
            all.extend(jobs.poll(usize::MAX));
            std::thread::yield_now();
        }
        all
    }

    #[test]
    fn builds_every_requested_tile() {
        let mut jobs = TileJobs::default();
        let coords: Vec<_> = (-1..=1).flat_map(|y| (-1..=1).map(move |x| TileCoord::new(x, y))).collect();

        jobs.start(&config(), &SplatRules::default(), Arc::new(FlatSource(5.0)), coords.clone());
        let built = drain(&mut jobs);

        let got: HashSet<_> = built.iter().map(|b| b.coord).collect();
        assert_eq!(got, coords.into_iter().collect());
        assert_eq!(jobs.progress(), Progress { done: 9, total: 9 });
        assert!(!jobs.is_busy());
        assert!(built.iter().all(|b| b.mesh.vertices.len() == 17 * 17));
    }

    #[test]
    fn restarting_discards_stale_results() {
        let mut jobs = TileJobs::default();
        let old: Vec<_> = (0..8).map(|x| TileCoord::new(x, 100)).collect();
        let new: Vec<_> = (0..3).map(|x| TileCoord::new(x, -100)).collect();

        jobs.start(&config(), &SplatRules::default(), Arc::new(FlatSource(1.0)), old);
        jobs.start(&config(), &SplatRules::default(), Arc::new(FlatSource(2.0)), new.clone());
        let built = drain(&mut jobs);

        assert!(built.iter().all(|b| b.coord.y == -100));
        assert_eq!(built.len(), new.len());
    }

    #[test]
    fn empty_request_is_immediately_done() {
        let mut jobs = TileJobs::default();
        jobs.start(&config(), &SplatRules::default(), Arc::new(FlatSource(0.0)), []);

        assert!(!jobs.is_busy());
        assert_eq!(jobs.elapsed(), Some(Duration::ZERO));
    }
}
