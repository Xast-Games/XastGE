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
