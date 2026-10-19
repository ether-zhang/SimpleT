// Native filter tuning adapted from WholeWindowLiquidGlass (MIT).
// Copyright (c) 2026 Daniil Nuzhdin. See third_party/WholeWindowLiquidGlass.LICENSE.
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
    sel, AllocAnyThread, DefinedClass, MainThreadOnly, Message,
};
use objc2_app_kit::NSView;
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSDictionary, NSKeyValueChangeKey,
    NSKeyValueObservingOptions, NSNumber, NSObject, NSObjectNSKeyValueCoding,
    NSObjectNSKeyValueObserverRegistration, NSObjectProtocol, NSString,
};
use objc2_quartz_core::{CALayer, CATransaction};

// Radius of our own Gaussian kernel, independent of glass material inputs.
const BACKGROUND_BLUR_RADIUS: f64 = 3.0;
const BACKGROUND_BLUR_NAME: &str = "simpletBackgroundBlur";
static LAYER_CONTEXT: u8 = 0;
static VIEW_CONTEXT: u8 = 0;

fn layer_context() -> *mut c_void {
    std::ptr::addr_of!(LAYER_CONTEXT).cast_mut().cast()
}

fn view_context() -> *mut c_void {
    std::ptr::addr_of!(VIEW_CONTEXT).cast_mut().cast()
}

struct LayerWatchState {
    root: RefCell<Option<Retained<CALayer>>>,
    layers: RefCell<Vec<Retained<CALayer>>>,
    refreshing: Cell<bool>,
    stopped: Cell<bool>,
    thread: std::thread::ThreadId,
}

define_class!(
    #[unsafe(super(NSObject))]
    // CALayer supports use on any one thread. Each watcher stays on the thread
    // where it was created; AppKit's glass hierarchy is confined to the UI thread.
    #[thread_kind = AllocAnyThread]
    #[name = "SimpleTBackgroundLayerWatch"]
    #[ivars = LayerWatchState]
    struct LayerWatch;

    unsafe impl NSObjectProtocol for LayerWatch {}

    impl LayerWatch {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn layer_changed(
            &self,
            path: Option<&NSString>,
            object: Option<&AnyObject>,
            change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            context: *mut c_void,
        ) {
            if context == layer_context() {
                // KVO runs before the native setter returns. Restore the filter
                // in this transaction instead of allowing a stock frame to render.
                self.refresh();
            } else {
                unsafe {
                    let _: () = msg_send![super(self),
                        observeValueForKeyPath: path,
                        ofObject: object,
                        change: change,
                        context: context,
                    ];
                }
            }
        }
    }
);

impl LayerWatch {
    fn new() -> Retained<Self> {
        let object = Self::alloc().set_ivars(LayerWatchState {
            root: RefCell::new(None),
            layers: RefCell::new(Vec::new()),
            refreshing: Cell::new(false),
            stopped: Cell::new(false),
            thread: std::thread::current().id(),
        });
        unsafe { msg_send![super(object), init] }
    }

    fn set_root(&self, root: Option<Retained<CALayer>>) {
        *self.ivars().root.borrow_mut() = root;
        self.refresh();
    }

    fn refresh(&self) {
        assert_eq!(self.ivars().thread, std::thread::current().id());
        if self.ivars().stopped.get() || self.ivars().refreshing.replace(true) {
            return;
        }
        struct RefreshGuard<'a>(&'a Cell<bool>);
        impl Drop for RefreshGuard<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _guard = RefreshGuard(&self.ivars().refreshing);
        let root = self.ivars().root.borrow().clone();
        let mut layers = Vec::new();
        if let Some(root) = &root {
            collect_layers(root, 0, &mut layers);
        }
        // Register before tuning, so asynchronous native replacements are also
        // covered. Nested notifications from our own correction are ignored.
        let previous = self.ivars().layers.replace(layers.clone());
        for layer in previous
            .iter()
            .filter(|layer| !contains_layer(&layers, layer))
        {
            self.unobserve(layer);
        }
        for layer in layers
            .iter()
            .filter(|layer| !contains_layer(&previous, layer))
        {
            for path in [ns_string!("filters"), ns_string!("sublayers")] {
                unsafe {
                    layer.addObserver_forKeyPath_options_context(
                        self,
                        path,
                        NSKeyValueObservingOptions::empty(),
                        layer_context(),
                    );
                }
            }
        }
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        let matched = root.as_ref().map_or(0, |root| tune_layers(root, 0));
        CATransaction::commit();
        #[cfg(debug_assertions)]
        record_refresh(matched, layers.len());
        #[cfg(not(debug_assertions))]
        let _ = matched;
    }

    fn unobserve(&self, layer: &CALayer) {
        for path in [ns_string!("filters"), ns_string!("sublayers")] {
            unsafe { layer.removeObserver_forKeyPath_context(self, path, layer_context()) };
        }
    }

    fn stop(&self) {
        if self.ivars().stopped.replace(true) {
            return;
        }
        let layers = self.ivars().layers.take();
        for layer in layers {
            self.unobserve(&layer);
        }
        self.ivars().root.take();
    }
}

impl Drop for LayerWatch {
    fn drop(&mut self) {
        self.stop();
    }
}

fn contains_layer(layers: &[Retained<CALayer>], layer: &CALayer) -> bool {
    layers
        .iter()
        .any(|candidate| std::ptr::eq(&**candidate, layer))
}

fn collect_layers(layer: &CALayer, depth: usize, layers: &mut Vec<Retained<CALayer>>) {
    if depth > 16 || layers.len() >= 256 || contains_layer(layers, layer) {
        return;
    }
    layers.push(layer.retain());
    if let Some(children) = unsafe { layer.sublayers() } {
        for child in children {
            collect_layers(&child, depth + 1, layers);
        }
    }
}

#[cfg(debug_assertions)]
fn record_refresh(matched: usize, observed: usize) {
    // Metadata only. The count lets native verification confirm corrections run
    // on material changes, without recording screen content or polling frames.
    thread_local! { static COUNT: Cell<u32> = const { Cell::new(0) }; }
    let changes = COUNT.with(|count| {
        let changes = count.get().saturating_add(1);
        count.set(changes);
        changes
    });
    let _ = std::fs::write(
        std::env::temp_dir().join("simplet-glass-blur.log"),
        format!(
            "pid={}\nmode=synchronous_layer_observation\nblur_radius={BACKGROUND_BLUR_RADIUS}\nmatched_backdrops={matched}\nobserved_layers={observed}\nrefreshes={changes}\n",
            std::process::id(),
        ),
    );
}

struct BlurState {
    view: Retained<NSView>,
    watch: Retained<LayerWatch>,
    observing: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SimpleTBackgroundBlurTuner"]
    #[ivars = BlurState]
    struct BlurTuner;

    unsafe impl NSObjectProtocol for BlurTuner {}

    impl BlurTuner {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn view_layer_changed(
            &self,
            path: Option<&NSString>,
            object: Option<&AnyObject>,
            change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            context: *mut c_void,
        ) {
            if context == view_context() {
                self.refresh();
            } else {
                unsafe {
                    let _: () = msg_send![super(self),
                        observeValueForKeyPath: path,
                        ofObject: object,
                        change: change,
                        context: context,
                    ];
                }
            }
        }
    }
);

impl BlurTuner {
    fn new(view: Retained<NSView>, mtm: MainThreadMarker) -> Retained<Self> {
        let object = Self::alloc(mtm).set_ivars(BlurState {
            view,
            watch: LayerWatch::new(),
            observing: Cell::new(false),
        });
        unsafe { msg_send![super(object), init] }
    }

    fn refresh(&self) {
        self.ivars().watch.set_root(self.ivars().view.layer());
    }

    fn unobserve_view(&self) {
        if self.ivars().observing.replace(false) {
            unsafe {
                self.ivars().view.removeObserver_forKeyPath_context(
                    self,
                    ns_string!("layer"),
                    view_context(),
                );
            }
        }
    }

    fn suspend(&self) {
        self.unobserve_view();
        self.ivars().watch.set_root(None);
    }

    fn resume(&self) {
        if !self.ivars().observing.replace(true) {
            unsafe {
                self.ivars().view.addObserver_forKeyPath_options_context(
                    self,
                    ns_string!("layer"),
                    NSKeyValueObservingOptions::empty(),
                    view_context(),
                );
            }
        }
        self.refresh();
    }

    fn stop(&self) {
        self.unobserve_view();
        self.ivars().watch.stop();
    }
}

impl Drop for BlurTuner {
    fn drop(&mut self) {
        self.stop();
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Retained<BlurTuner>>> = const { RefCell::new(None) };
}

pub fn clear() {
    ACTIVE.with_borrow_mut(|active| {
        if let Some(tuner) = active.take() {
            tuner.stop();
        }
    });
}

pub fn install(view: Retained<NSView>) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("背景模糊必须在主线程更新")?;
    clear();
    let tuner = BlurTuner::new(view, mtm);
    // Observe the view's root too: AppKit may replace that layer, rather than
    // only its children. There is no display link, repeated timer or frame poll.
    tuner.resume();
    ACTIVE.with_borrow_mut(|active| *active = Some(tuner));
    Ok(())
}

pub fn set_visible(visible: bool) {
    ACTIVE.with_borrow(|active| {
        if let Some(tuner) = active {
            if visible {
                tuner.resume();
            }
        }
    });
}

// Suspend only after the native window has actually hidden. The closing
// animation still needs synchronous corrections for its entire visible span.
pub fn suspend_hidden() {
    ACTIVE.with_borrow(|active| {
        if let Some(tuner) = active {
            tuner.suspend();
        }
    });
}

fn set_number(filter: &NSObject, key: &NSString, value: f64) -> bool {
    if filter.valueForKey(key).is_some_and(|current| {
        current
            .downcast_ref::<NSNumber>()
            .is_some_and(|number| (number.as_f64() - value).abs() < 0.001)
    }) {
        return false;
    }
    // Every key has been checked against this filter's advertised inputKeys.
    unsafe { filter.setValue_forKey(Some(&NSNumber::new_f64(value)), key) };
    true
}

fn set_filter_number(layer: &CALayer, filter: &NSObject, key: &NSString, value: f64) -> bool {
    if !set_number(filter, key, value) {
        return false;
    }
    // Publish through the named layer key path as well: mutating a CAFilter
    // object alone need not invalidate WindowServer's cached filter inputs.
    if filter.respondsToSelector(sel!(name)) {
        if let Some(name) = filter.valueForKey(ns_string!("name")) {
            if let Some(name) = name.downcast_ref::<NSString>() {
                let name = name.to_string();
                if !name.is_empty()
                    && name.len() <= 128
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                {
                    let path = NSString::from_str(&format!("filters.{name}.{key}"));
                    unsafe { layer.setValue_forKeyPath(Some(&NSNumber::new_f64(value)), &path) };
                }
            }
        }
    }
    true
}

fn input_keys(filter: &NSObject) -> Vec<String> {
    if !filter.respondsToSelector(sel!(inputKeys)) {
        return Vec::new();
    }
    let Some(inputs) = filter.valueForKey(ns_string!("inputKeys")) else {
        return Vec::new();
    };
    let Some(inputs) = inputs.downcast_ref::<NSArray>() else {
        return Vec::new();
    };
    inputs
        .iter()
        .filter_map(|key| key.downcast_ref::<NSString>().map(|key| key.to_string()))
        .collect()
}

fn is_background_blur(filter: &NSObject) -> bool {
    filter.respondsToSelector(sel!(name))
        && filter.valueForKey(ns_string!("name")).is_some_and(|name| {
            name.downcast_ref::<NSString>()
                .is_some_and(|name| name.to_string() == BACKGROUND_BLUR_NAME)
        })
}

fn new_background_blur() -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"CAFilter")?;
    class.class_method(sel!(filterWithType:))?;
    let value: Option<Retained<AnyObject>> =
        unsafe { msg_send![class, filterWithType: ns_string!("gaussianBlur")] };
    let value = value?;
    let filter = value.downcast_ref::<NSObject>()?;
    let keys = input_keys(filter);
    if !keys.iter().any(|key| key == "inputRadius") || !filter.respondsToSelector(sel!(setName:)) {
        return None;
    }
    unsafe {
        let _: () = msg_send![filter, setName: &*NSString::from_str(BACKGROUND_BLUR_NAME)];
    }
    set_number(filter, ns_string!("inputRadius"), BACKGROUND_BLUR_RADIUS);
    if keys.iter().any(|key| key == "inputNormalizeEdges") {
        unsafe {
            filter.setValue_forKey(
                Some(&NSNumber::new_bool(true)),
                ns_string!("inputNormalizeEdges"),
            )
        };
    }
    Some(value)
}

fn tune_layers(layer: &CALayer, depth: usize) -> usize {
    if depth > 16 {
        return 0;
    }
    let mut matched = 0;
    if let Some(filters) = layer.filters() {
        let is_backdrop =
            AnyClass::get(c"CABackdropLayer").is_some_and(|class| layer.isKindOfClass(class));
        let is_glass_background = filters.iter().any(|filter| {
            filter.downcast_ref::<NSObject>().is_some_and(|filter| {
                input_keys(filter)
                    .iter()
                    .any(|key| key == "inputBlurRadius")
            })
        });
        if is_backdrop || is_glass_background {
            let mut updated = filters.to_vec();
            if let Some(filter) = updated.iter().find_map(|value| {
                value
                    .downcast_ref::<NSObject>()
                    .filter(|filter| is_background_blur(filter))
            }) {
                matched += 1;
                if set_filter_number(
                    layer,
                    filter,
                    ns_string!("inputRadius"),
                    BACKGROUND_BLUR_RADIUS,
                ) {
                    unsafe { layer.setFilters(Some(&NSArray::from_retained_slice(&updated))) };
                }
            } else if let Some(filter) = new_background_blur() {
                // A separate spatial Gaussian pass blurs the sampled image even
                // when the glass material's own progressive blur is subdued.
                // Preserve all of AppKit's original material and color filters.
                updated.push(filter);
                unsafe { layer.setFilters(Some(&NSArray::from_retained_slice(&updated))) };
                matched += 1;
            }
        }
    }
    // AppKit owns this hierarchy; callbacks run on the UI thread and only read
    // its CALayer children. Tests supply an isolated, owned layer hierarchy.
    if let Some(children) = unsafe { layer.sublayers() } {
        for child in children {
            matched += tune_layers(&child, depth + 1);
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(kind: &NSString) -> Retained<AnyObject> {
        let class = AnyClass::get(c"CAFilter").expect("Core Animation filter class");
        unsafe { msg_send![class, filterWithType: kind] }
    }

    fn number(filter: &AnyObject, key: &NSString) -> f64 {
        filter
            .downcast_ref::<NSObject>()
            .unwrap()
            .valueForKey(key)
            .unwrap()
            .downcast_ref::<NSNumber>()
            .unwrap()
            .as_f64()
    }

    fn backdrop() -> Retained<CALayer> {
        let class = AnyClass::get(c"CABackdropLayer").expect("Native backdrop layer");
        unsafe { msg_send![class, new] }
    }

    fn assert_background_radius(layer: &CALayer) {
        let radius = layer
            .valueForKeyPath(ns_string!("filters.simpletBackgroundBlur.inputRadius"))
            .expect("custom blur is present before the native setter returns");
        assert_eq!(
            radius.downcast_ref::<NSNumber>().unwrap().as_f64(),
            BACKGROUND_BLUR_RADIUS,
        );
    }

    #[test]
    fn material_replacements_are_repaired_synchronously_without_a_frame_callback() {
        let backdrop = backdrop();
        let watch = LayerWatch::new();
        watch.set_root(Some(backdrop.clone()));
        for _ in 0..100 {
            let stock = filter(ns_string!("gaussianBlur"));
            unsafe { backdrop.setFilters(Some(&NSArray::from_slice(&[&*stock]))) };
            // No refresh, timer, run loop or frame callback between replacement
            // and assertion: the stock-only state must not reach a rendered frame.
            assert_background_radius(&backdrop);
            assert_eq!(backdrop.filters().unwrap().len(), 2);
        }
    }

    #[test]
    fn hidden_layers_are_unobserved_and_repaired_synchronously_before_reveal() {
        let backdrop = backdrop();
        let stock = filter(ns_string!("gaussianBlur"));
        let watch = LayerWatch::new();
        watch.set_root(Some(backdrop.clone()));
        for _ in 0..20 {
            watch.set_root(None);
            assert!(watch.ivars().layers.borrow().is_empty());
            unsafe { backdrop.setFilters(Some(&NSArray::from_slice(&[&*stock]))) };
            assert_eq!(backdrop.filters().unwrap().len(), 1);
            watch.set_root(Some(backdrop.clone()));
            assert_background_radius(&backdrop);
            assert_eq!(backdrop.filters().unwrap().len(), 2);
        }
    }

    #[test]
    fn new_backdrops_are_observed_on_attachment_and_detached_layers_are_released() {
        let root = CALayer::layer();
        let watch = LayerWatch::new();
        watch.set_root(Some(root.clone()));
        let backdrop = backdrop();
        let stock = filter(ns_string!("gaussianBlur"));
        unsafe { backdrop.setFilters(Some(&NSArray::from_slice(&[&*stock]))) };
        root.addSublayer(&backdrop);
        assert_background_radius(&backdrop);

        backdrop.removeFromSuperlayer();
        assert_eq!(watch.ivars().layers.borrow().len(), 1);
        unsafe { backdrop.setFilters(Some(&NSArray::from_slice(&[&*stock]))) };
        assert_eq!(backdrop.filters().unwrap().len(), 1);
    }

    #[test]
    fn changing_root_or_stopping_balances_observers_and_restores_native_ownership() {
        let previous = backdrop();
        let current = backdrop();
        let stock = filter(ns_string!("gaussianBlur"));
        let watch = LayerWatch::new();
        watch.set_root(Some(previous.clone()));
        watch.set_root(Some(current.clone()));
        unsafe {
            previous.setFilters(Some(&NSArray::from_slice(&[&*stock])));
            current.setFilters(Some(&NSArray::from_slice(&[&*stock])));
        }
        assert_eq!(previous.filters().unwrap().len(), 1);
        assert_background_radius(&current);

        watch.stop();
        watch.stop();
        drop(watch);
        unsafe { current.setFilters(Some(&NSArray::from_slice(&[&*stock]))) };
        assert_eq!(current.filters().unwrap().len(), 1);
    }

    #[test]
    fn native_backdrop_radius_changes_without_blurring_foreground_or_changing_saturation() {
        let class = AnyClass::get(c"CABackdropLayer").expect("Native backdrop layer");
        let backdrop: Retained<CALayer> = unsafe { msg_send![class, new] };
        let blur = filter(ns_string!("gaussianBlur"));
        let saturate = filter(ns_string!("colorSaturate"));
        let foreground_blur = filter(ns_string!("gaussianBlur"));
        unsafe {
            let _: () = msg_send![&*blur, setName: ns_string!("simpletBackgroundTest")];
        }
        set_number(blur.downcast_ref().unwrap(), ns_string!("inputRadius"), 1.0);
        set_number(
            saturate.downcast_ref().unwrap(),
            ns_string!("inputAmount"),
            1.12,
        );
        set_number(
            foreground_blur.downcast_ref().unwrap(),
            ns_string!("inputRadius"),
            3.0,
        );
        let root = CALayer::layer();
        unsafe {
            backdrop.setFilters(Some(&NSArray::from_slice(&[&*blur, &*saturate])));
            root.setFilters(Some(&NSArray::from_slice(&[&*foreground_blur])));
        }
        root.addSublayer(&backdrop);

        assert_eq!(tune_layers(&root, 0), 1);
        assert_eq!(number(&blur, ns_string!("inputRadius")), 1.0);
        let published = backdrop
            .valueForKeyPath(ns_string!("filters.simpletBackgroundBlur.inputRadius"))
            .unwrap();
        assert_eq!(
            published.downcast_ref::<NSNumber>().unwrap().as_f64(),
            BACKGROUND_BLUR_RADIUS
        );
        assert_eq!(number(&foreground_blur, ns_string!("inputRadius")), 3.0);
        assert!((number(&saturate, ns_string!("inputAmount")) - 1.12).abs() < 0.001);
        assert_eq!(backdrop.filters().unwrap().len(), 3);
        assert_eq!(tune_layers(&root, 0), 1);
        assert_eq!(backdrop.filters().unwrap().len(), 3);
        // Simulate AppKit rebuilding its stock material after a focus change.
        unsafe { backdrop.setFilters(Some(&NSArray::from_slice(&[&*blur, &*saturate]))) };
        assert_eq!(tune_layers(&root, 0), 1);
        assert_eq!(backdrop.filters().unwrap().len(), 3);
    }
}
