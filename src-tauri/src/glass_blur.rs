// Native filter tuning adapted from WholeWindowLiquidGlass (MIT).
// Copyright (c) 2026 Daniil Nuzhdin. See third_party/WholeWindowLiquidGlass.LICENSE.
use std::cell::{Cell, RefCell};

use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
    sel, DefinedClass, MainThreadOnly,
};
use objc2_app_kit::NSView;
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSNumber, NSObject, NSObjectNSKeyValueCoding,
    NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes, NSString,
};
use objc2_quartz_core::{CADisplayLink, CAFrameRateRange, CALayer, CATransaction};

// Radius of our own Gaussian kernel, independent of glass material inputs.
const BACKGROUND_BLUR_RADIUS: f64 = 3.0;
const BACKGROUND_BLUR_NAME: &str = "simpletBackgroundBlur";

struct BlurState {
    view: Retained<NSView>,
    link: RefCell<Option<Retained<CADisplayLink>>>,
    reported: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SimpleTBackgroundBlurTuner"]
    #[ivars = BlurState]
    struct BlurTuner;

    unsafe impl NSObjectProtocol for BlurTuner {}

    impl BlurTuner {
        #[unsafe(method(updateBlur:))]
        fn update_blur(&self, _link: &CADisplayLink) {
            self.refresh();
        }
    }
);

impl BlurTuner {
    fn new(view: Retained<NSView>, mtm: MainThreadMarker) -> Retained<Self> {
        let object = Self::alloc(mtm).set_ivars(BlurState {
            view,
            link: RefCell::new(None),
            reported: Cell::new(false),
        });
        unsafe { msg_send![super(object), init] }
    }

    fn refresh(&self) {
        let Some(layer) = self.ivars().view.layer() else {
            return;
        };
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        let matched = tune_layers(&layer, 0);
        CATransaction::commit();
        if matched > 0 && !self.ivars().reported.replace(true) {
            // Diagnostic metadata only: no screen pixels or user text.
            #[cfg(debug_assertions)]
            let _ = std::fs::write(
                std::env::temp_dir().join("simplet-glass-blur.log"),
                format!(
                    "pid={}\nfilter_type=gaussianBlur\nblur_radius={BACKGROUND_BLUR_RADIUS}\nmatched_backdrops={matched}\n",
                    std::process::id(),
                ),
            );
        }
    }

    fn stop(&self) {
        if let Some(link) = self.ivars().link.borrow_mut().take() {
            // CADisplayLink retains its target; break that cycle before removing
            // the native glass view or switching back to the default appearance.
            link.invalidate();
        }
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
    #[cfg(debug_assertions)]
    let _ = std::fs::write(
        std::env::temp_dir().join("simplet-glass-blur.log"),
        format!(
            "pid={}\nstage=waiting_for_native_filters\n",
            std::process::id()
        ),
    );
    let tuner = BlurTuner::new(view, mtm);
    // Native glass constructs and replaces its internal filters asynchronously.
    // A view-bound display link tracks those replacements while it is onscreen.
    let link = unsafe {
        tuner
            .ivars()
            .view
            .displayLinkWithTarget_selector(&tuner, sel!(updateBlur:))
    };
    link.setPreferredFrameRateRange(CAFrameRateRange::new(30.0, 30.0, 30.0));
    unsafe {
        link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes);
    }
    *tuner.ivars().link.borrow_mut() = Some(link);
    tuner.refresh();
    ACTIVE.with_borrow_mut(|active| *active = Some(tuner));
    Ok(())
}

pub fn set_visible(visible: bool) {
    ACTIVE.with_borrow(|active| {
        if let Some(tuner) = active {
            if let Some(link) = tuner.ivars().link.borrow().as_ref() {
                link.setPaused(!visible);
            }
            if visible {
                tuner.refresh();
            }
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
