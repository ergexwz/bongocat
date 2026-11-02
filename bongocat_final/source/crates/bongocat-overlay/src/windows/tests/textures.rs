//! The GPU structs matching the layout D3D11 expects.

use super::*;

/// Exercise the actual shaders and blend state on WARP, including faint edges,
/// overlapping mask sources and full coverage. Compare the R8 coverage byte
/// with the previous BGRA alpha byte rather than only checking descriptors.
#[test]
fn single_channel_masks_match_bgra_alpha_on_the_gpu() {
    // SAFETY: the test owns all D3D resources on this thread. Upload buffers
    // outlive synchronous creation, and mapped bytes are read before Unmap.
    unsafe {
        let (device, context) = try_create_d3d11_device(D3D_DRIVER_TYPE_WARP).unwrap();
        let pipelines = create_pipelines(&device).unwrap();
        let vertices = quad_vertices(ModelBounds {
            min_x: -1.0,
            min_y: -1.0,
            max_x: 1.0,
            max_y: 1.0,
        });
        let vertex =
            Some(create_buffer(&device, &vertices, D3D11_BIND_VERTEX_BUFFER.0 as u32).unwrap());
        let indices = create_buffer(
            &device,
            &[0_u16, 1, 2, 0, 2, 3],
            D3D11_BIND_INDEX_BUFFER.0 as u32,
        )
        .unwrap();
        let uniforms = Uniforms {
            scale_offset: [1.0, 1.0, 0.0, 0.0],
            multiply_color: [1.0; 4],
            screen_color: [0.0; 4],
            mask_settings: [0.0; 4],
            corner_radius: [0.0; 4],
            opacity: 1.0,
            padding: [0.0; 3],
        };
        context.UpdateSubresource(
            &pipelines.constant_buffer,
            0,
            None,
            std::ptr::from_ref(&uniforms).cast(),
            0,
            0,
        );
        context.VSSetConstantBuffers(0, Some(&[Some(pipelines.constant_buffer.clone())]));
        context.VSSetShader(&pipelines.vertex_shader, None);
        context.PSSetShader(&pipelines.mask_shader, None);
        context.PSSetSamplers(0, Some(&[Some(pipelines.sampler.clone())]));
        context.IASetInputLayout(&pipelines.input_layout);
        context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context.IASetVertexBuffers(
            0,
            1,
            Some(&raw const vertex),
            Some(&(size_of::<bongocat_render::Vertex>() as u32)),
            Some(&0),
        );
        context.IASetIndexBuffer(&indices, DXGI_FORMAT_R16_UINT, 0);
        context.RSSetState(&pipelines.rasterizer);
        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
            Width: 8.0,
            Height: 8.0,
            MaxDepth: 1.0,
            ..Default::default()
        }]));
        context.OMSetBlendState(&pipelines.mask_blend, None, u32::MAX);
        for alpha in [0_u8, 1, 64, 128, 254, 255] {
            let source = create_texture_resource(
                &device,
                1,
                1,
                MODEL_TEXTURE_FORMAT,
                [0, 0, 0, alpha].as_ptr(),
                4,
            )
            .unwrap();
            context.PSSetShaderResources(0, Some(&[Some(source.shader_resource.clone()), None]));
            let mut coverage = Vec::new();
            for format in [MASK_TEXTURE_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM] {
                let descriptor = D3D11_TEXTURE2D_DESC {
                    Width: 8,
                    Height: 8,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: format,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                    ..Default::default()
                };
                let mut texture = None;
                device
                    .CreateTexture2D(&descriptor, None, Some(&mut texture))
                    .unwrap();
                let texture = texture.unwrap();
                let target = create_render_target(&device, &texture, format).unwrap();
                context.OMSetRenderTargets(
                    Some(&[Some(target.clone())]),
                    None::<&ID3D11DepthStencilView>,
                );
                context.ClearRenderTargetView(&target, &[0.0; 4]);
                for _ in 0..3 {
                    context.DrawIndexed(6, 0, 0);
                }
                context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
                let staging = create_staging_texture(&device, &texture).unwrap();
                context.CopyResource(&staging, &texture);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context
                    .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                    .unwrap();
                let offset = 3 * mapped.RowPitch as usize
                    + if format == MASK_TEXTURE_FORMAT {
                        2
                    } else {
                        2 * 4 + 3
                    };
                coverage.push(*mapped.pData.cast::<u8>().add(offset));
                context.Unmap(&staging, 0);
            }
            assert_eq!(coverage[0], coverage[1], "source alpha {alpha}");
            let a = f32::from(alpha) / 255.0;
            let expected = ((1.0 - (1.0 - a).powi(3)) * 255.0).round() as u8;
            assert!(
                coverage[0].abs_diff(expected) <= 1,
                "coverage {coverage:?}, expected {expected}"
            );
        }
        context.ClearState();
    }
}

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
