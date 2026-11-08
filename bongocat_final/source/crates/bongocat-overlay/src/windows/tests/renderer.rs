//! Fitting a model into the window without distorting it.

use super::*;

#[test]
#[ignore = "requires an isolated interactive Windows graphics process"]
fn replacement_windows_reuse_the_device_without_growing_handles() {
    let _apartment = ComApartment::initialize().unwrap();
    let handles = || {
        let mut count = 0;
        // SAFETY: only reads a count for the current process.
        unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) }.unwrap();
        count
    };
    let catalog = PresetModelCatalog::open(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/models"),
        ModelPackageLimits::default(),
    )
    .unwrap();
    let (runtime, consumer) = RuntimeOwner::start_with_rendering(false, 64);
    let client = runtime.client();
    client.wait_for_revision(1, RUNTIME_TIMEOUT).unwrap();
    let model = catalog.load(&ModelId::parse("standard").unwrap()).unwrap();
    let (_token, frame) = prepare_switch_frame(
        &client,
        &consumer,
        Arc::new(model),
        Arc::new(preview_input_bindings("standard")),
    )
    .unwrap();
    let options = OverlaySessionOptions::default();
    let mut overlay = NativeOverlay::create(&frame, options, None, None, None).unwrap();
    let expected = overlay.draw_capturing(true).unwrap();
    let mut warm_handles = None;
    for iteration in 0..40 {
        let mut candidate = NativeOverlay::create_inner(
            &frame,
            options,
            Some(overlay.window.bounds().unwrap()),
            None,
            None,
            Some(&overlay.renderer),
        )
        .unwrap();
        assert_eq!(candidate.renderer.device, overlay.renderer.device);
        assert_eq!(candidate.renderer.context, overlay.renderer.context);
        assert_eq!(candidate.draw_capturing(true).unwrap(), expected);
        if iteration % 2 == 0 {
            // A rejected candidate clears the shared context on drop. The old
            // renderer must fully restore its bindings on its next draw.
            drop(candidate);
        } else {
            // A committed candidate must also draw after the old renderer drops.
            overlay = candidate;
        }
        assert_eq!(overlay.draw_capturing(true).unwrap(), expected);
        if iteration % 8 == 7 {
            thread::sleep(Duration::from_millis(50));
            let current = handles();
            eprintln!("replacement_handles iteration={iteration} count={current}");
            let warm = *warm_handles.get_or_insert(current);
            assert!(current <= warm + 8, "handles grew from {warm} to {current}");
        }
    }
    drop(overlay);
    runtime.shutdown(RUNTIME_TIMEOUT).unwrap();
}

#[test]
fn model_transform_preserves_aspect_ratio() {
    let canvas = CanvasInfo {
        width: 2048.0,
        height: 2048.0,
        origin_x: 1024.0,
        origin_y: 1024.0,
        pixels_per_unit: 1024.0,
    };
    assert_eq!(
        model_transform(ModelBounds::from_canvas(canvas), 800.0, 800.0, false),
        [1.0, 1.0, -0.0, -0.0]
    );
    assert_eq!(
        model_transform(ModelBounds::from_canvas(canvas), 1600.0, 800.0, false),
        [0.5, 1.0, -0.0, -0.0]
    );
    assert_eq!(
        model_transform(ModelBounds::from_canvas(canvas), 800.0, 800.0, true),
        [-1.0, 1.0, 0.0, -0.0]
    );
}
