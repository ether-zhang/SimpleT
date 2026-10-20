use objc2::{msg_send, rc::Retained, runtime::AnyClass, MainThreadOnly};
use objc2_app_kit::{
    NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
};
use objc2_core_graphics::CGColor;
use objc2_foundation::{ns_string, MainThreadMarker, NSPoint, NSRect, NSValue};
use objc2_quartz_core::{
    CABasicAnimation, CALayer, CAMediaTiming, CAMediaTimingFunction, CATransaction,
};

use super::{
    glass_blur,
    widgets::{self, rect},
};

pub struct Glass {
    root: Retained<NSView>,
    reveal_root: Retained<NSView>,
    frost: Option<Retained<NSVisualEffectView>>,
    glass: Option<Retained<NSGlassEffectView>>,
    regions: Vec<Retained<CALayer>>,
    mask: Retained<CALayer>,
}

pub struct Surface {
    pub bounds: NSRect,
    pub clip: NSRect,
    pub radius: f64,
}

impl Surface {
    pub fn new(bounds: NSRect, radius: f64) -> Self {
        Self {
            bounds,
            clip: bounds,
            radius,
        }
    }
}

// The reveal host is deliberately unflipped. The top edge (y + height) stays
// fixed below the tray throughout both opening and closing animations.
fn reveal_rect(width: f64, height: f64, visible: bool) -> NSRect {
    let shown_height = if visible {
        (height - 12.0).max(0.0)
    } else {
        0.0
    };
    rect(
        6.0,
        height - 6.0 - shown_height,
        (width - 12.0).max(0.0),
        shown_height,
    )
}

impl Glass {
    pub fn new(root: Retained<NSView>, reveal_root: Retained<NSView>) -> Self {
        Self {
            root,
            reveal_root,
            frost: None,
            glass: None,
            regions: Vec::new(),
            mask: CALayer::layer(),
        }
    }

    pub fn set(&mut self, enabled: bool, mtm: MainThreadMarker) {
        widgets::GLASS.set(enabled);
        if let Some(view) = self.frost.take() {
            view.removeFromSuperview();
        }
        if let Some(view) = self.glass.take() {
            view.removeFromSuperview();
        }
        glass_blur::clear();
        for layer in self.regions.drain(..) {
            layer.removeFromSuperlayer();
        }
        self.root.setWantsLayer(true);
        if let Some(layer) = self.root.layer() {
            let fill = if enabled {
                widgets::color(0.27, 0.28, 0.37, 0.045)
            } else {
                NSColor::windowBackgroundColor()
            };
            layer.setBackgroundColor(Some(&fill.CGColor()));
            layer.setBorderWidth(if enabled { 0.0 } else { 1.0 });
            layer.setBorderColor(Some(&NSColor::separatorColor().CGColor()));
            layer.setCornerRadius(14.0);
            layer.setMasksToBounds(true);
        }
        if !enabled {
            return;
        }
        let bounds = self.root.bounds();
        let frost = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), bounds);
        frost.setMaterial(NSVisualEffectMaterial::Popover);
        frost.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        frost.setState(NSVisualEffectState::Active);
        frost.setWantsLayer(true);
        self.root.addSubview_positioned_relativeTo(
            &frost,
            objc2_app_kit::NSWindowOrderingMode::Below,
            None,
        );
        if let Some(class) = AnyClass::get(c"NSGlassEffectView") {
            // Dynamic lookup keeps the Popover fallback usable on older macOS.
            let glass: Retained<NSGlassEffectView> = unsafe { msg_send![class, new] };
            glass.setFrame(bounds);
            glass.setCornerRadius(14.0);
            glass.setStyle(NSGlassEffectViewStyle::Clear);
            self.root.addSubview_positioned_relativeTo(
                &glass,
                objc2_app_kit::NSWindowOrderingMode::Below,
                None,
            );
            let _ = glass_blur::install(Retained::into_super(glass.clone()));
            self.glass = Some(glass);
        }
        self.mask.setFrame(bounds);
        self.mask.setGeometryFlipped(frost.isFlipped());
        self.mask.setBackgroundColor(Some(&CGColor::new_srgb(
            1.0,
            1.0,
            1.0,
            if self.glass.is_some() { 0.0 } else { 0.775 },
        )));
        if let Some(layer) = frost.layer() {
            unsafe {
                layer.setMask(Some(&self.mask));
            }
        }
        self.frost = Some(frost);
    }

    pub fn surfaces(&mut self, surfaces: &[Surface]) {
        let Some(frost) = self.frost.as_ref() else {
            return;
        };
        let bounds = self.root.bounds();
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        for (index, surface) in surfaces.iter().enumerate() {
            if index >= self.regions.len() {
                let layer = CALayer::layer();
                layer.addSublayer(&CALayer::layer());
                self.mask.addSublayer(&layer);
                self.regions.push(layer);
            }
            // NSView conversion handles the flipped card and unflipped native
            // material once. Keep a full rounded shape inside a rectangular
            // viewport clip so partial history rows do not gain extra corners.
            let area = self.root.convertRect_toView(surface.bounds, Some(frost));
            let clip = self.root.convertRect_toView(surface.clip, Some(frost));
            let x = area.origin.x.max(clip.origin.x).max(0.0);
            let y = area.origin.y.max(clip.origin.y).max(0.0);
            let right = (area.origin.x + area.size.width)
                .min(clip.origin.x + clip.size.width)
                .min(bounds.size.width);
            let top = (area.origin.y + area.size.height)
                .min(clip.origin.y + clip.size.height)
                .min(bounds.size.height);
            let container = &self.regions[index];
            container.setFrame(rect(x, y, (right - x).max(0.0), (top - y).max(0.0)));
            container.setMasksToBounds(true);
            if let Some(control) =
                unsafe { container.sublayers() }.and_then(|layers| layers.firstObject())
            {
                control.setFrame(rect(
                    area.origin.x - x,
                    area.origin.y - y,
                    area.size.width,
                    area.size.height,
                ));
                control.setCornerRadius(surface.radius);
                control.setBackgroundColor(Some(&CGColor::new_srgb(1.0, 1.0, 1.0, 1.0)));
            }
        }
        for layer in self.regions.drain(surfaces.len()..) {
            layer.removeFromSuperlayer();
        }
        CATransaction::commit();
    }

    pub fn reveal(&self, visible: bool, animated: bool) {
        if visible {
            glass_blur::set_visible(true);
        }
        self.reveal_root.setWantsLayer(true);
        let Some(layer) = self.reveal_root.layer() else {
            return;
        };
        let mask = layer.mask().unwrap_or_else(|| {
            let mask = CALayer::layer();
            mask.setAnchorPoint(NSPoint::new(0.0, 0.0));
            mask.setBackgroundColor(Some(&CGColor::new_srgb(1.0, 1.0, 1.0, 1.0)));
            unsafe {
                layer.setMask(Some(&mask));
            }
            mask
        });
        let bounds = self.reveal_root.bounds();
        let frame = reveal_rect(bounds.size.width, bounds.size.height, visible);
        let target = rect(0.0, 0.0, frame.size.width, frame.size.height);
        let position = frame.origin;
        let presentation = if animated {
            unsafe { mask.presentationLayer() }
        } else {
            None
        };
        let from = presentation.as_deref().unwrap_or(&mask);
        let from_bounds = from.bounds();
        let from_position = from.position();
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        mask.removeAllAnimations();
        mask.setBounds(target);
        mask.setPosition(position);
        mask.setCornerRadius(14.0);
        mask.setMasksToBounds(true);
        if animated {
            let (from_bounds, to_bounds, from_position, to_position) = unsafe {
                (
                    NSValue::valueWithRect(from_bounds),
                    NSValue::valueWithRect(target),
                    NSValue::valueWithPoint(from_position),
                    NSValue::valueWithPoint(position),
                )
            };
            let animation =
                |key, from: &objc2::runtime::AnyObject, to: &objc2::runtime::AnyObject| {
                    let animation = CABasicAnimation::animationWithKeyPath(Some(key));
                    unsafe {
                        animation.setFromValue(Some(from));
                        animation.setToValue(Some(to));
                    }
                    animation.setDuration(0.28);
                    animation.setTimingFunction(Some(
                        &CAMediaTimingFunction::functionWithControlPoints(0.22, 1.0, 0.36, 1.0),
                    ));
                    animation
                };
            mask.addAnimation_forKey(
                &animation(ns_string!("bounds"), &from_bounds, &to_bounds),
                Some(ns_string!("revealBounds")),
            );
            mask.addAnimation_forKey(
                &animation(ns_string!("position"), &from_position, &to_position),
                Some(ns_string!("revealPosition")),
            );
        }
        CATransaction::commit();
    }
}

#[cfg(test)]
mod tests {
    use super::reveal_rect;

    #[test]
    fn tray_reveal_keeps_the_top_edge_fixed_in_both_directions() {
        let shown = reveal_rect(756.0, 483.0, true);
        let hidden = reveal_rect(756.0, 483.0, false);
        assert_eq!(shown.origin.y + shown.size.height, 477.0);
        assert_eq!(hidden.origin.y + hidden.size.height, 477.0);
        for step in 0..=10 {
            let t = step as f64 / 10.0;
            let y = hidden.origin.y * (1.0 - t) + shown.origin.y * t;
            let height = shown.size.height * t;
            assert!((y + height - 477.0).abs() < 0.00001);
        }
    }
}
