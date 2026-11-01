//! The GPU structs matching the layout D3D11 expects.

use super::*;

#[test]
fn gpu_structs_match_d3d11_layout() {
    assert_eq!(size_of::<bongocat_render::Vertex>(), 16);
    assert_eq!(size_of::<Uniforms>(), 96);
    assert_eq!(size_of::<Uniforms>() % 16, 0);
}

#[test]
fn texture_upload_premultiplies_rgb_before_filtering() {
    let mut image = image::RgbaImage::from_raw(2, 1, vec![200, 100, 50, 128, 20, 40, 60, 0])
        .expect("test image dimensions");

    premultiply_alpha(&mut image);

    assert_eq!(image.as_raw(), &[100, 50, 25, 128, 0, 0, 0, 0]);
}

#[test]
fn key_crop_keeps_faint_edges_padding_and_original_coordinates() {
    let mut image = image::RgbaImage::new(100, 80);
    image.put_pixel(20, 10, image::Rgba([255, 0, 0, 1]));
    image.put_pixel(29, 19, image::Rgba([0, 255, 0, 255]));
    let crop = key_alpha_crop(&image);
    assert_eq!(crop, (18, 8, 14, 14));
    let bounds = ModelBounds {
        min_x: -50.0,
        max_x: 50.0,
        min_y: -40.0,
        max_y: 40.0,
    };
    assert_eq!(
        cropped_key_bounds(bounds, 100, 80, crop),
        ModelBounds {
            min_x: -32.0,
            max_x: -18.0,
            min_y: 18.0,
            max_y: 32.0,
        }
    );
    // Every original pixel is reconstructed at exactly the same canvas pixel.
    let cropped = image::imageops::crop_imm(&image, crop.0, crop.1, crop.2, crop.3).to_image();
    let mut restored = image::RgbaImage::new(100, 80);
    image::imageops::replace(
        &mut restored,
        &cropped,
        i64::from(crop.0),
        i64::from(crop.1),
    );
    assert_eq!(image, restored);
}

#[test]
fn key_crop_handles_empty_and_edge_to_edge_images() {
    let mut image = image::RgbaImage::new(12, 9);
    assert_eq!(key_alpha_crop(&image), (0, 0, 1, 1));
    image.put_pixel(0, 0, image::Rgba([0, 0, 0, 255]));
    image.put_pixel(11, 8, image::Rgba([0, 0, 0, 255]));
    assert_eq!(key_alpha_crop(&image), (0, 0, 12, 9));
}

#[test]
fn all_builtin_key_images_keep_their_visible_pixels_and_use_less_texture_storage() {
    use std::io::Write;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/models");
    for model in ["standard", "keyboard", "gamepad"] {
        let mut before = 0u64;
        let mut after = 0u64;
        let mut count = 0;
        for side in ["left-keys", "right-keys"] {
            let directory = root.join(model).join("resources").join(side);
            if !directory.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(directory).unwrap() {
                let path = file.unwrap().path();
                if path.extension().and_then(|s| s.to_str()) != Some("png") {
                    continue;
                }
                let image = image::open(path).unwrap().into_rgba8();
                let (x, y, w, h) = key_alpha_crop(&image);
                before += u64::from(image.width()) * u64::from(image.height()) * 4;
                after += u64::from(w) * u64::from(h) * 4;
                count += 1;
                let cropped = image::imageops::crop_imm(&image, x, y, w, h).to_image();
                for (px, py, pixel) in image.enumerate_pixels().filter(|(_, _, p)| p[3] != 0) {
                    assert!(px >= x && px < x + w && py >= y && py < y + h);
                    assert_eq!(pixel, cropped.get_pixel(px - x, py - y));
                }
            }
        }
        assert!(count > 0 && after < before, "{model}");
        writeln!(
            std::io::stdout(),
            "key textures {model}: count={count}, rgba_before={before}, rgba_after={after}"
        )
        .unwrap();
    }
}
