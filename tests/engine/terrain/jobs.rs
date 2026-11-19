use std::collections::HashSet;

use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use xastge::terrain::jobs::*;
use xastge::terrain::*;
use xastge::terrain::source::FlatSource;

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
