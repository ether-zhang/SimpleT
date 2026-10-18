// A single content-view mask reveals the native frost and WebView together.
// The material stays installed between openings; no compositor changes at rest.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct ClipRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Surface {
    rect: ClipRect,
    clip: ClipRect,
    radius: f64,
}

#[cfg(any(target_os = "macos", test))]
fn material_surfaces(surfaces: &[Surface], width: f64, height: f64) -> Vec<Surface> {
    surfaces
        .iter()
        .take(128)
        .filter_map(|surface| {
            let rect = &surface.rect;
            let clip = &surface.clip;
            if [
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                clip.x,
                clip.y,
                clip.width,
                clip.height,
                surface.radius,
            ]
            .iter()
            .any(|value| !value.is_finite() || value.abs() > 100_000.0)
                || rect.width <= 0.0
                || rect.height <= 0.0
                || clip.width <= 0.0
                || clip.height <= 0.0
            {
                return None;
            }
            let x = rect.x.max(clip.x).max(0.0);
            let y = rect.y.max(clip.y).max(0.0);
            let right = (rect.x + rect.width).min(clip.x + clip.width).min(width);
            let bottom = (rect.y + rect.height).min(clip.y + clip.height).min(height);
            (right > x && bottom > y).then(|| Surface {
                rect: rect.clone(),
                clip: ClipRect {
                    x,
                    y,
                    width: right - x,
                    height: bottom - y,
                },
                radius: surface
                    .radius
                    .max(0.0)
                    .min(rect.width.min(rect.height) / 2.0),
            })
        })
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn native_rect(rect: &ClipRect, height: f64, flipped: bool) -> ClipRect {
    ClipRect {
        x: rect.x,
        y: if flipped {
            rect.y
        } else {
            height - rect.y - rect.height
        },
        width: rect.width,
        height: rect.height,
    }
}

#[cfg(any(target_os = "macos", test))]
fn clip_rect(width: f64, height: f64, flipped: bool, visible: bool) -> ClipRect {
    ClipRect {
        x: 6.0,
        y: if visible || flipped {
            6.0
        } else {
            height - 6.0
        },
        width: (width - 12.0).max(0.0),
        height: if visible {
            (height - 12.0).max(0.0)
        } else {
            0.0
        },
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{rc::Retained, ClassType};
    use objc2_app_kit::{NSView, NSVisualEffectView};
    use objc2_core_graphics::CGColor;
    use objc2_foundation::{
        ns_string, MainThreadMarker, NSObjectProtocol, NSPoint, NSRect, NSSize, NSValue,
    };
    use objc2_quartz_core::{
        CABasicAnimation, CALayer, CAMediaTiming, CAMediaTimingFunction, CATransaction,
    };

    // Control opacity compensates for the background already beneath its mask.
    const BACKGROUND_FROST_ALPHA: f64 = 0.775;
    const CONTROL_FROST_ALPHA: f64 = 1.0;

    fn content_view(window: &tauri::WebviewWindow) -> Result<&NSView, String> {
        MainThreadMarker::new().ok_or("磨砂材质必须在主线程更新")?;
        let pointer = window.ns_view().map_err(|error| error.to_string())?;
        // Tauri owns this view. All callers borrow it only on the main thread.
        unsafe { (pointer as *const NSView).as_ref() }.ok_or("窗口视图不可用".into())
    }

    pub fn set_appearance(window: &tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
        use window_vibrancy::{
            apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
        };
        let root = content_view(window)?;
        // Unlike Window::set_effects, these calls finish synchronously on the UI
        // thread, so material installation is complete before the panel shows.
        clear_vibrancy(window).map_err(|error| error.to_string())?;
        if !enabled {
            if let Some(layer) = root.layer() {
                // The independent mask has no reference back to the content layer.
                unsafe { layer.setMask(None) };
            }
            return Ok(());
        }
        apply_vibrancy(
            window,
            NSVisualEffectMaterial::Popover,
            Some(NSVisualEffectState::Active),
            Some(14.0),
        )
        .map_err(|error| error.to_string())?;
        let bounds = root.bounds();
        let mut found = false;
        for view in root.subviews() {
            if view.isKindOfClass(NSVisualEffectView::class()) {
                view.setFrame(NSRect::new(
                    NSPoint::new(bounds.origin.x + 6.0, bounds.origin.y + 6.0),
                    NSSize::new(bounds.size.width - 12.0, bounds.size.height - 12.0),
                ));
                view.setAlphaValue(1.0);
                found = true;
            }
        }
        if !found {
            return Err("窗口磨砂材质不可用".into());
        }
        update_surfaces(window, &[])?;
        // Switching appearance in an already-visible panel must keep it visible.
        set_reveal(window, true, false)
    }

    pub fn update_surfaces(
        window: &tauri::WebviewWindow,
        surfaces: &[super::Surface],
    ) -> Result<(), String> {
        let root = content_view(window)?;
        let white = CGColor::new_srgb(1.0, 1.0, 1.0, 1.0);
        let background = CGColor::new_srgb(1.0, 1.0, 1.0, BACKGROUND_FROST_ALPHA);
        for view in root.subviews() {
            if !view.isKindOfClass(NSVisualEffectView::class()) {
                continue;
            }
            view.setWantsLayer(true);
            let layer = view.layer().ok_or("磨砂图层不可用")?;
            let bounds = layer.bounds();
            let flipped = layer.isGeometryFlipped();
            let mask = CALayer::layer();
            mask.setFrame(bounds);
            mask.setGeometryFlipped(flipped);
            mask.setBackgroundColor(Some(&background));
            let controls = CALayer::layer();
            controls.setFrame(bounds);
            controls.setGeometryFlipped(flipped);
            // Flatten overlapping controls before opacity is applied, so menus
            // over a text pane do not accumulate a second layer of native frost.
            controls.setAllowsGroupOpacity(true);
            controls.setOpacity(
                ((CONTROL_FROST_ALPHA - BACKGROUND_FROST_ALPHA) / (1.0 - BACKGROUND_FROST_ALPHA))
                    as f32,
            );
            for surface in super::material_surfaces(surfaces, bounds.size.width, bounds.size.height)
            {
                let rect = super::native_rect(&surface.rect, bounds.size.height, flipped);
                let clip = super::native_rect(&surface.clip, bounds.size.height, flipped);
                // Clip partial history rows at the scrolling viewport while keeping
                // their original rounded corners, rather than rounding the cut edge.
                let container = CALayer::layer();
                container.setFrame(NSRect::new(
                    NSPoint::new(clip.x, clip.y),
                    NSSize::new(clip.width, clip.height),
                ));
                container.setGeometryFlipped(flipped);
                container.setMasksToBounds(true);
                let control = CALayer::layer();
                control.setFrame(NSRect::new(
                    NSPoint::new(rect.x - clip.x, rect.y - clip.y),
                    NSSize::new(rect.width, rect.height),
                ));
                control.setCornerRadius(surface.radius);
                control.setBackgroundColor(Some(&white));
                container.addSublayer(&control);
                controls.addSublayer(&container);
            }
            mask.addSublayer(&controls);
            CATransaction::begin();
            CATransaction::setDisableActions(true);
            // All mask layers are independent and have no reference back to the view.
            unsafe { layer.setMask(Some(&mask)) };
            CATransaction::commit();
        }
        Ok(())
    }

    fn animation(
        key: &objc2_foundation::NSString,
        from: &objc2::runtime::AnyObject,
        to: &objc2::runtime::AnyObject,
    ) -> Retained<CABasicAnimation> {
        let animation = CABasicAnimation::animationWithKeyPath(Some(key));
        // Values are NSValue boxes matching the selected bounds/position property.
        unsafe {
            animation.setFromValue(Some(from));
            animation.setToValue(Some(to));
        }
        animation.setDuration(0.28);
        animation.setTimingFunction(Some(&CAMediaTimingFunction::functionWithControlPoints(
            0.22, 1.0, 0.36, 1.0,
        )));
        animation
    }

    pub fn set_reveal(
        window: &tauri::WebviewWindow,
        visible: bool,
        animated: bool,
    ) -> Result<(), String> {
        let root = content_view(window)?;
        root.setWantsLayer(true);
        let layer = root.layer().ok_or("窗口裁切图层不可用")?;
        let mask = layer.mask().unwrap_or_else(|| {
            let mask = CALayer::layer();
            mask.setAnchorPoint(NSPoint::new(0.0, 0.0));
            mask.setBackgroundColor(Some(&CGColor::new_srgb(0.0, 0.0, 0.0, 1.0)));
            // A mask cannot have a superlayer or reference the layer it masks.
            unsafe { layer.setMask(Some(&mask)) };
            mask
        });
        let bounds = root.bounds();
        let rect = super::clip_rect(
            bounds.size.width,
            bounds.size.height,
            layer.isGeometryFlipped(),
            visible,
        );
        let target_bounds =
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(rect.width, rect.height));
        let target_position = NSPoint::new(rect.x, rect.y);
        // Read the current presentation geometry before replacing an interrupted
        // animation. Model values already equal the final state when it completes.
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
        mask.setBounds(target_bounds);
        mask.setPosition(target_position);
        if animated {
            // These boxes contain the Core Animation bounds and position types.
            let (from_bounds, to_bounds, from_position, to_position) = unsafe {
                (
                    NSValue::valueWithRect(from_bounds),
                    NSValue::valueWithRect(target_bounds),
                    NSValue::valueWithPoint(from_position),
                    NSValue::valueWithPoint(target_position),
                )
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
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use macos::{set_appearance, set_reveal, update_surfaces};

#[cfg(test)]
mod tests {
    use super::{clip_rect, material_surfaces, native_rect, ClipRect, Surface};

    #[test]
    fn surfaces_preserve_rounded_geometry_and_clip_to_the_visible_card() {
        let surface = Surface {
            rect: ClipRect {
                x: -10.0,
                y: 80.0,
                width: 300.0,
                height: 180.0,
            },
            clip: ClipRect {
                x: 0.0,
                y: 120.0,
                width: 744.0,
                height: 200.0,
            },
            radius: 999.0,
        };
        let regions = material_surfaces(&[surface.clone()], 744.0, 471.0);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].rect, surface.rect);
        assert_eq!(
            regions[0].clip,
            ClipRect {
                x: 0.0,
                y: 120.0,
                width: 290.0,
                height: 140.0
            }
        );
        assert_eq!(regions[0].radius, 90.0);
        let mut invalid = surface.clone();
        invalid.radius = f64::NAN;
        let mut outside = surface;
        outside.clip.y = 500.0;
        assert!(material_surfaces(&[invalid, outside], 744.0, 471.0).is_empty());
    }

    #[test]
    fn surface_coordinates_match_top_down_webview_rectangles() {
        let rect = ClipRect {
            x: 15.0,
            y: 63.0,
            width: 301.0,
            height: 393.0,
        };
        assert_eq!(native_rect(&rect, 471.0, true), rect);
        assert_eq!(native_rect(&rect, 471.0, false).y, 15.0);
    }

    #[test]
    fn expanded_mask_matches_the_card_on_both_coordinate_systems() {
        for flipped in [false, true] {
            let rect = clip_rect(756.0, 483.0, flipped, true);
            assert_eq!(
                (rect.x, rect.y, rect.width, rect.height),
                (6.0, 6.0, 744.0, 471.0)
            );
        }
    }

    #[test]
    fn collapsed_mask_starts_at_the_menu_bar_edge() {
        let normal = clip_rect(756.0, 483.0, false, false);
        let flipped = clip_rect(756.0, 483.0, true, false);
        assert_eq!((normal.y, normal.height), (477.0, 0.0));
        assert_eq!((flipped.y, flipped.height), (6.0, 0.0));
    }
}
