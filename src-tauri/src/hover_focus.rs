use std::cell::{Cell, RefCell};

use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, Bool},
    sel, AllocAnyThread, DefinedClass, Encode, MainThreadOnly,
};
use objc2_app_kit::{
    NSApplication, NSEvent, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow,
    NSWindowDidBecomeKeyNotification, NSWindowDidResignKeyNotification,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSRunLoop,
    NSRunLoopCommonModes, NSTimer,
};
use tauri::Manager;

const HOVER_DELAY: f64 = 0.12;

macro_rules! trace {
    ($($argument:tt)*) => {
        #[cfg(debug_assertions)]
        trace_line(format!($($argument)*));
    };
}

#[cfg(debug_assertions)]
fn trace_line(line: String) {
    thread_local! {
        static LINES: RefCell<std::collections::VecDeque<String>> = const { RefCell::new(std::collections::VecDeque::new()) };
    }
    LINES.with_borrow_mut(|lines| {
        if lines.len() == 64 {
            lines.pop_front();
        }
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        // Bounded metadata only; no user text or screen contents.
        lines.push_back(format!("pid={} t={elapsed} {line}", std::process::id()));
        let _ = std::fs::write(
            std::env::temp_dir().join("simplet-hover-focus.log"),
            lines.iter().cloned().collect::<Vec<_>>().join("\n"),
        );
    });
}

#[derive(Default)]
struct HoverIntent {
    inside: bool,
    pending: bool,
}

impl HoverIntent {
    fn enter(&mut self, inside: bool) -> bool {
        let entered = inside && !self.inside;
        self.inside = inside;
        if entered {
            self.pending = true;
        }
        entered
    }

    fn cancel(&mut self, inside: bool) {
        self.inside = inside;
        self.pending = false;
    }

    fn take(&mut self) -> bool {
        std::mem::take(&mut self.pending) && self.inside
    }
}

struct HoverState {
    window: tauri::WebviewWindow,
    view: Retained<NSView>,
    intent: RefCell<HoverIntent>,
    timer: RefCell<Option<Retained<NSTimer>>>,
    generation: Cell<u32>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SimpleTHoverFocusOwner"]
    #[ivars = HoverState]
    struct HoverFocusOwner;

    unsafe impl NSObjectProtocol for HoverFocusOwner {}

    impl HoverFocusOwner {
        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.enter();
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            // An exit may be synthesized while geometrically inside, for
            // example when another window covers the panel. Never focus here.
            trace!("exit inside={}", pointer_inside(&self.ivars().view));
            self.cancel();
        }

        #[unsafe(method(windowFocusChanged:))]
        fn window_focus_changed(&self, _notification: &NSNotification) {
            #[cfg(debug_assertions)]
            if let Some(native) = self.ivars().view.window() {
                let app = NSApplication::sharedApplication(self.mtm());
                trace!(
                    "focus-change key={} cps={:?} appearance={:?} inside={} class={:?}",
                    native.isKeyWindow(), has_key_focus(&app), active_appearance(&native),
                    pointer_inside(&self.ivars().view), native.class().name(),
                );
            }
            // A focus transition cancels an older hover request; it never
            // schedules a new one or changes the WebView's input responder.
            self.cancel();
        }

        #[unsafe(method(hoverDelayElapsed:))]
        fn hover_delay_elapsed(&self, timer: &NSTimer) {
            let current = self.ivars().timer.borrow().as_ref()
                .is_some_and(|current| std::ptr::eq(&**current, timer));
            if !current {
                return;
            }
            // Break the timer/target retain cycle before changing native focus.
            self.ivars().timer.borrow_mut().take();
            let requested = self.ivars().intent.borrow_mut().take();
            if requested {
                restore_focus(&self.ivars().window, self.ivars().generation.get());
            }
        }
    }
);

impl HoverFocusOwner {
    fn new(
        window: tauri::WebviewWindow,
        view: Retained<NSView>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let inside = pointer_inside(&view);
        let object = Self::alloc(mtm).set_ivars(HoverState {
            window,
            view,
            intent: RefCell::new(HoverIntent {
                inside,
                pending: false,
            }),
            timer: RefCell::new(None),
            generation: Cell::new(0),
        });
        unsafe { msg_send![super(object), init] }
    }

    fn cancel(&self) {
        if let Some(timer) = self.ivars().timer.borrow_mut().take() {
            timer.invalidate();
        }
        self.ivars()
            .intent
            .borrow_mut()
            .cancel(pointer_inside(&self.ivars().view));
    }

    fn enter(&self) {
        let inside = pointer_inside(&self.ivars().view);
        let fresh = self.ivars().intent.borrow_mut().enter(inside);
        trace!("enter inside={inside} fresh={fresh}");
        if !fresh {
            return;
        }
        let Some(generation) = hover_generation(&self.ivars().window) else {
            self.cancel();
            return;
        };
        self.ivars().generation.set(generation);
        // One cancellable delay per entry, not a repeating timer or mouse poll.
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                HOVER_DELAY,
                self,
                sel!(hoverDelayElapsed:),
                None,
                false,
            )
        };
        if let Some(previous) = self.ivars().timer.borrow_mut().replace(timer.clone()) {
            previous.invalidate();
        }
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
    }
}

fn pointer_inside(view: &NSView) -> bool {
    let Some(native) = view.window() else {
        return false;
    };
    if !native.isVisible() {
        return false;
    }
    let point = view.convertPoint_fromView(
        native.convertPointFromScreen(NSEvent::mouseLocation()),
        None,
    );
    // Bounds remain stable during key-window/appearance changes.
    let bounds = view.bounds();
    point.x >= bounds.origin.x
        && point.y >= bounds.origin.y
        && point.x < bounds.origin.x + bounds.size.width
        && point.y < bounds.origin.y + bounds.size.height
}

fn hover_generation(window: &tauri::WebviewWindow) -> Option<u32> {
    window
        .state::<crate::AppState>()
        .flyout
        .lock()
        .ok()
        .and_then(|state| state.can_hover_focus().then_some(state.generation))
}

fn has_key_focus(app: &NSApplication) -> Option<bool> {
    let method = app.class().instance_method(sel!(_hasKeyFocus))?;
    if method.arguments_count() != 2
        || method.return_type().to_bytes() != Bool::ENCODING.to_string().as_bytes()
    {
        return None;
    }
    let focused: Bool = unsafe { msg_send![app, _hasKeyFocus] };
    Some(focused.as_bool())
}

#[cfg(debug_assertions)]
fn active_appearance(window: &NSWindow) -> Option<bool> {
    let method = window.class().instance_method(sel!(_hasActiveAppearance))?;
    if method.arguments_count() != 2
        || method.return_type().to_bytes() != Bool::ENCODING.to_string().as_bytes()
    {
        return None;
    }
    let active: Bool = unsafe { msg_send![window, _hasActiveAppearance] };
    Some(active.as_bool())
}

fn restore_focus(window: &tauri::WebviewWindow, generation: u32) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if hover_generation(window) != Some(generation) {
        trace!("skip stale-or-closing generation={generation}");
        return;
    }
    let Ok(pointer) = window.ns_view() else {
        return;
    };
    let Some(view) = (unsafe { pointer.cast::<NSView>().as_ref() }) else {
        return;
    };
    let Some(native) = view.window() else {
        return;
    };
    if !AnyClass::get(c"FlyoutPanel").is_some_and(|class| native.isKindOfClass(class))
        || !pointer_inside(view)
    {
        trace!("skip pointer-outside-or-not-panel");
        return;
    }
    let app = NSApplication::sharedApplication(mtm);
    if native.isKeyWindow() && has_key_focus(&app) == Some(true) {
        trace!("skip already-focused");
        return;
    }
    // Native tracking handles occlusion; auxiliary glass windows can make a
    // separate window-number equality check reject a valid tracking event.
    trace!(
        "restore-before key={} cps={:?} appearance={:?}",
        native.isKeyWindow(),
        has_key_focus(&app),
        active_appearance(&native),
    );
    let webview: &tauri::Webview = window.as_ref();
    let focused = webview.set_focus();
    native.makeKeyWindow();
    trace!(
        "restore-after generation={generation} responder-ok={} key={} cps={:?} appearance={:?}",
        focused.is_ok(),
        native.isKeyWindow(),
        has_key_focus(&app),
        active_appearance(&native),
    );
    let _ = focused;
}

struct HoverTracking {
    view: Retained<NSView>,
    area: Retained<NSTrackingArea>,
    owner: Retained<HoverFocusOwner>,
}

impl Drop for HoverTracking {
    fn drop(&mut self) {
        self.owner.cancel();
        self.view.removeTrackingArea(&self.area);
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(&self.owner) };
    }
}

thread_local! {
    static TRACKING: RefCell<Option<HoverTracking>> = const { RefCell::new(None) };
}

pub fn install(window: &tauri::WebviewWindow) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("鼠标跟踪必须在主线程安装")?;
    let pointer = window.ns_view().map_err(|error| error.to_string())?;
    // Tauri owns this view. Installation follows panel conversion/material setup.
    let view = unsafe { Retained::retain(pointer.cast::<NSView>()) }.ok_or("窗口视图不可用")?;
    if TRACKING.with_borrow(|tracking| {
        tracking
            .as_ref()
            .is_some_and(|tracking| std::ptr::eq(&*tracking.view, &*view))
    }) {
        return Ok(());
    }
    let native = view.window().ok_or("窗口不可用")?;
    let owner = HoverFocusOwner::new(window.clone(), view.clone(), mtm);
    // No mouseMoved callback and no changes to acceptsMouseMovedEvents.
    let options = NSTrackingAreaOptions::MouseEnteredAndExited
        | NSTrackingAreaOptions::ActiveAlways
        | NSTrackingAreaOptions::InVisibleRect;
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            view.bounds(),
            options,
            Some(&owner),
            None,
        )
    };
    let center = NSNotificationCenter::defaultCenter();
    unsafe {
        for notification in [
            NSWindowDidBecomeKeyNotification,
            NSWindowDidResignKeyNotification,
        ] {
            center.addObserver_selector_name_object(
                &owner,
                sel!(windowFocusChanged:),
                Some(notification),
                Some(&native),
            );
        }
    }
    TRACKING.with_borrow_mut(|tracking| {
        *tracking = None;
        view.addTrackingArea(&area);
        *tracking = Some(HoverTracking { view, area, owner });
    });
    trace!(
        "installed entry-exit tracking class={:?}",
        native.class().name()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::HoverIntent;

    #[test]
    fn exit_never_schedules_focus_even_when_pointer_is_geometrically_inside() {
        let mut intent = HoverIntent::default();
        intent.cancel(true);
        assert!(!intent.take());
        assert!(!intent.enter(true));
        intent.cancel(false);
        assert!(intent.enter(true));
        assert!(intent.take());
        assert!(!intent.take());
    }

    #[test]
    fn losing_focus_or_leaving_cancels_an_already_scheduled_hover() {
        let mut intent = HoverIntent::default();
        assert!(intent.enter(true));
        intent.cancel(false);
        assert!(!intent.take());
        assert!(intent.enter(true));
        intent.cancel(true);
        assert!(!intent.take());
        assert!(!intent.enter(true));
    }

    #[test]
    fn stale_entry_outside_cannot_schedule_and_real_reentry_fires_once() {
        let mut intent = HoverIntent::default();
        assert!(!intent.enter(false));
        assert!(!intent.take());
        assert!(intent.enter(true));
        assert!(!intent.enter(true));
        assert!(intent.take());
        intent.cancel(false);
        assert!(intent.enter(true));
    }
}
