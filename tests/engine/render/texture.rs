use image::RgbaImage;
use xastge::render::texture::*;

#[test]
fn mip_chain_halves_down_to_one_pixel() {
    let chain = mip_chain(&RgbaImage::new(16, 4));
    let sizes: Vec<_> = chain.iter().map(|m| (m.width(), m.height())).collect();
    assert_eq!(sizes, vec![(16, 4), (8, 2), (4, 1), (2, 1), (1, 1)]);
}
