use xastge::terrain::splat::*;
use xastge::terrain::*;

#[test]
fn band_ramps_smoothly_between_its_ends() {
    let band = Band::new(10.0, 20.0);
    assert_eq!(band.ramp(5.0), 0.0);
    assert_eq!(band.ramp(25.0), 1.0);
    assert!((band.ramp(15.0) - 0.5).abs() < 1e-6);
}

#[test]
fn weights_always_sum_to_one() {
    let rules = SplatRules::default();
    for height in [-50.0, 0.0, 200.0, 350.0, 900.0] {
        for slope in [0.0, 0.1, 0.25, 0.4, 0.9] {
            for concavity in [-0.1, 0.0, 0.05, 0.2] {
                for noise in [-1.0, 0.0, 1.0] {
                    let w = rules.weights(height, slope, concavity, 0.5, noise);
                    assert!((w.sum() - 1.0).abs() < 1e-5, "{w:?}");
                }
            }
        }
    }
}

#[test]
fn layers_follow_the_terrain() {
    let rules = SplatRules::default();
    assert_eq!(rules.weights(50.0, 0.0, 0.0, 0.0, 0.0).pack(), [255, 0, 0, 0]);
    assert_eq!(rules.weights(50.0, 0.0, 0.5, 0.0, 0.0).pack()[1], 255);
    assert_eq!(rules.weights(50.0, 0.0, 0.0, 1.0, 0.0).pack()[1], 255);
    assert!(rules.weights(50.0, 0.12, 0.0, 0.0, 0.0).pack()[2] > 0);
    assert_eq!(rules.weights(2000.0, 0.0, 0.0, 0.0, 0.0).pack()[2], 255);
    assert_eq!(rules.weights(50.0, 0.8, 0.0, 0.0, 0.0).pack()[3], 255);
}
