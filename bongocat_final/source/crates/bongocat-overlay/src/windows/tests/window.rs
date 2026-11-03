//! The window class, its taskbar button, and correcting a saved box that left
//! the desktop.

use super::*;
use bongocat_runtime::OverlaySettings;

#[test]
fn window_mode_uses_model_client_size_when_switching_from_saved_pet_geometry() {
    let canvas = CanvasInfo {
        width: 2048.0,
        height: 1188.0,
        origin_x: 1024.0,
        origin_y: 594.0,
        pixels_per_unit: 1024.0,
    };
    let saved = OverlayWindowBounds::new(100, 100, 720, 308);
    let pet = OverlayWindow::create(
        OverlaySessionOptions::default(),
        canvas,
        Some(saved),
        None,
        None,
    )
    .unwrap();
    let expected = crate::model_switch_window_bounds(saved, canvas);
    assert_eq!(pet.bounds().unwrap(), expected);
    for scale_percent in [25, 100, 200] {
        let window = OverlayWindow::create(
            OverlaySessionOptions {
                window_mode: true,
                scale_percent,
                ..OverlaySessionOptions::default()
            },
            canvas,
            Some(pet.bounds().unwrap()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(window.bounds().unwrap(), expected);
        // SAFETY: this test owns the HWND and reads both rectangles on its
        // creation thread. The title bar/border must be outside the model size.
        unsafe {
            let frame = window_frame_rect(window.hwnd, expected.width, expected.height).unwrap();
            let mut outer = RECT::default();
            GetWindowRect(window.hwnd, &mut outer).unwrap();
            assert_eq!(outer.right - outer.left, frame.right - frame.left);
            assert_eq!(outer.bottom - outer.top, frame.bottom - frame.top);
            assert!(outer.right - outer.left > expected.width as i32);
            assert!(outer.bottom - outer.top > expected.height as i32);
        }
    }
}

#[test]
fn overlay_window_class_supports_overlapping_replacement_windows() {
    let canvas = CanvasInfo {
        width: 2048.0,
        height: 2048.0,
        origin_x: 1024.0,
        origin_y: 1024.0,
        pixels_per_unit: 1024.0,
    };
    let first = OverlayWindow::create(OverlaySessionOptions::default(), canvas, None, None, None)
        .expect("create first overlay window");
    let second = OverlayWindow::create(OverlaySessionOptions::default(), canvas, None, None, None)
        .expect("reuse class for replacement overlay window");

    assert_ne!(first.hwnd, second.hwnd);
    drop(first);
    drop(second);
}

#[test]
fn overlay_window_creation_corrects_a_saved_box_off_the_desktop() {
    let screens = screen_bounds_all();
    let left_most = screens
        .iter()
        .min_by_key(|screen| screen.x)
        .expect("at least one display");
    let canvas = CanvasInfo {
        width: 2_048.0,
        height: 2_048.0,
        origin_x: 1_024.0,
        origin_y: 1_024.0,
        pixels_per_unit: 1_024.0,
    };
    let candidate = OverlayWindowBounds::new(left_most.x - 10_000, left_most.y, 350, 350);
    let expected = correction_for_screens(&screens, candidate).expect("a display to correct into");
    let window = OverlayWindow::create(
        OverlaySessionOptions::default(),
        canvas,
        Some(candidate),
        None,
        None,
    )
    .expect("create constrained overlay window");
    let created = window.bounds().expect("constrained bounds");
    assert_eq!(created, expected);
    assert!(bounds_inside_screens(&screen_bounds_all(), created));
}

/// The `show_taskbar_icon` preference owns the model window's taskbar button,
/// and the window is created with the saved value rather than being corrected
/// after its first frame.
#[test]
fn overlay_window_owns_its_taskbar_button_at_creation() {
    let canvas = CanvasInfo {
        width: 2048.0,
        height: 2048.0,
        origin_x: 1024.0,
        origin_y: 1024.0,
        pixels_per_unit: 1024.0,
    };
    let shown = OverlayWindow::create(
        OverlaySessionOptions {
            taskbar_icon_visible: true,
            ..OverlaySessionOptions::default()
        },
        canvas,
        None,
        None,
        None,
    )
    .expect("create overlay window with a taskbar button");
    assert!(shown.taskbar_icon_is_visible());

    let hidden = OverlayWindow::create(
        OverlaySessionOptions {
            taskbar_icon_visible: false,
            ..OverlaySessionOptions::default()
        },
        canvas,
        None,
        None,
        None,
    )
    .expect("create overlay window without a taskbar button");
    assert!(!hidden.taskbar_icon_is_visible());
}

/// The pet must always be a layered top-level window, not only while
/// click-through is on.
///
/// Click-through only adds the pointer-transparent bit; it must not be the
/// condition that makes the pet a layered window. Fullscreen games can still
/// use presentation paths that cover the overlay.
#[test]
fn a_pet_window_is_layered_so_dwm_composes_it_over_fullscreen_games() {
    let canvas = CanvasInfo {
        width: 2048.0,
        height: 1188.0,
        origin_x: 1024.0,
        origin_y: 594.0,
        pixels_per_unit: 1024.0,
    };
    let window = OverlayWindow::create(OverlaySessionOptions::default(), canvas, None, None, None)
        .expect("create the model window");
    // SAFETY: the test owns the live HWND and reads its extended style on the
    // creation thread.
    let style = WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(window.hwnd, GWL_EXSTYLE) } as u32);
    assert!(
        style.contains(WS_EX_LAYERED),
        "the pet window must stay in DWM's layered composition path"
    );
    assert!(
        !style.contains(WS_EX_TRANSPARENT),
        "click-through is off by default, so the window must not eat input"
    );
    assert!(style.contains(WS_EX_TOPMOST));
    assert!(style.contains(WS_EX_NOREDIRECTIONBITMAP));
}

/// Toggling the taskbar button in place must survive a model switch, which
/// replaces the HWND, so the applied value is recorded on the session options
/// the replacement window is created from.
#[test]
fn taskbar_toggle_preserves_unrelated_extended_styles_and_is_idempotent() {
    let canvas = CanvasInfo {
        width: 2048.0,
        height: 2048.0,
        origin_x: 1024.0,
        origin_y: 1024.0,
        pixels_per_unit: 1024.0,
    };
    let window = OverlayWindow::create(
        OverlaySessionOptions {
            click_through: true,
            ..OverlaySessionOptions::default()
        },
        canvas,
        None,
        None,
        None,
    )
    .expect("create overlay window");
    let click_through = WS_EX_LAYERED.0 as isize | WS_EX_TRANSPARENT.0 as isize;
    // SAFETY: the test owns the live HWND and reads its extended style on the
    // creation thread.
    let unrelated = unsafe { GetWindowLongPtrW(window.hwnd, GWL_EXSTYLE) } & click_through;
    assert_eq!(unrelated, click_through);

    window
        .set_taskbar_icon_visible(true)
        .expect("give the model window a taskbar button");
    assert!(window.taskbar_icon_is_visible());
    // A repeat is a no-op rather than a second style write.
    window
        .set_taskbar_icon_visible(true)
        .expect("repeat the taskbar button write");
    // SAFETY: the test owns the live HWND and reads its extended style on the
    // creation thread.
    let style = unsafe { GetWindowLongPtrW(window.hwnd, GWL_EXSTYLE) };
    assert_eq!(style & click_through, click_through);
    assert_eq!(
        style & WS_EX_NOACTIVATE.0 as isize,
        WS_EX_NOACTIVATE.0 as isize
    );
    assert_eq!(
        style & WS_EX_NOREDIRECTIONBITMAP.0 as isize,
        WS_EX_NOREDIRECTIONBITMAP.0 as isize
    );

    window
        .set_taskbar_icon_visible(false)
        .expect("take the model window's taskbar button away");
    assert!(!window.taskbar_icon_is_visible());
    // SAFETY: the test owns the live HWND and reads its extended style on the
    // creation thread.
    let style = unsafe { GetWindowLongPtrW(window.hwnd, GWL_EXSTYLE) };
    assert_eq!(style & click_through, click_through);
}

/// The session keeps the applied value in its options, which is what a
/// replacement window is created from, so a model switch cannot bring back a
/// taskbar button the user turned off.
#[test]
fn session_options_carry_the_taskbar_button_into_a_replacement_window() {
    // The shipped default leaves the button off, so the fallback options must
    // not hand out a taskbar button no configuration asked for.
    assert!(!OverlaySessionOptions::default().taskbar_icon_visible);
    let options = OverlaySessionOptions {
        taskbar_icon_visible: false,
        ..OverlaySessionOptions::default()
    };
    // A runtime overlay settings change carries no taskbar field, so the
    // applied value has to survive the projection onto the options.
    let projected = options.with_runtime_settings(OverlaySettings::default());
    assert!(!projected.taskbar_icon_visible);
    // The taskbar button is applied in place, so it never forces the window to
    // be replaced.
    assert!(!options.requires_window_recreation(projected));
}

/// "Always on top" has to be a property of the window, not a value written once.
///
/// user32 documents the rule that decides membership in the topmost band: a
/// topmost window that is repositioned after any non-topmost window is no longer
/// topmost. Nothing reports that transition, and a window that left the band
/// looks exactly like one whose setting was turned off — it sits behind the next
/// window the user opens. The guard is the recovery path, so the test drives the
/// demotion user32 performs and asserts the window comes back.
#[test]
fn showing_a_demoted_window_restores_the_users_topmost_preference() {
    let canvas = CanvasInfo {
        width: 2048.0,
        height: 1188.0,
        origin_x: 1024.0,
        origin_y: 594.0,
        pixels_per_unit: 1024.0,
    };
    let mut window =
        OverlayWindow::create(OverlaySessionOptions::default(), canvas, None, None, None).unwrap();
    assert!(window.is_topmost());
    // SAFETY: this test owns the live HWND; only its z-order is changed.
    unsafe {
        SetWindowPos(
            window.hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )
        .unwrap();
    }
    assert!(!window.is_topmost());
    window.show().unwrap();
    window.maintain_topmost().unwrap();
    assert!(window.is_topmost());
    assert!(window.is_always_on_top());
    window.set_always_on_top(false).unwrap();
    window.set_always_on_top(true).unwrap();
    assert!(window.is_topmost());
}
