//! Pen pressure for macOS. WebKit reports tablet pens as a plain mouse without pressure, so the app
//! watches the native mouse events: when one comes from a tablet, its pressure is forwarded to the
//! webview as a `pen-pressure` event (see src/app/canvas/pressure.ts). Windows doesn't need this:
//! WebView2 delivers pen pressure through Pointer Events.

#[cfg(target_os = "macos")]
pub fn install<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventSubtype, NSEventType};
    use tauri::Emitter;

    let app = app.clone();
    let mask = NSEventMask::LeftMouseDown | NSEventMask::LeftMouseDragged | NSEventMask::TabletPoint;
    let block = RcBlock::new(move |ev: NonNull<NSEvent>| -> *mut NSEvent {
        let e = unsafe { ev.as_ref() };
        let ty = e.r#type();
        let from_tablet = ty == NSEventType::TabletPoint
            || ((ty == NSEventType::LeftMouseDown || ty == NSEventType::LeftMouseDragged) && e.subtype() == NSEventSubtype::TabletPoint);
        if from_tablet {
            let _ = app.emit("pen-pressure", e.pressure());
        }
        ev.as_ptr() // pass the event on unchanged
    });
    // The monitor and its block live as long as the app.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
    std::mem::forget(monitor);
    std::mem::forget(block);
}

#[cfg(not(target_os = "macos"))]
pub fn install<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) {}
