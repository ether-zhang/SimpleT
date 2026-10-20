use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

use objc2::{
    define_class, msg_send, rc::Retained, runtime::AnyObject, AnyThread, DefinedClass,
    MainThreadOnly,
};
use objc2_app_kit::*;
use objc2_core_graphics::{CGContext, CGPath};
use objc2_foundation::{
    MainThreadMarker, NSAffineTransform, NSArray, NSDictionary, NSObjectProtocol, NSPoint, NSRect,
    NSSize, NSString,
};
use objc2_quartz_core::CATransaction;

use super::{post, Action};

thread_local! {
    pub static GLASS: Cell<bool> = const { Cell::new(false) };
    static FONTS: RefCell<BTreeMap<(u16, FontWeight), Retained<NSFont>>> = const { RefCell::new(BTreeMap::new()) };
}

pub fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

pub fn color(r: f64, g: f64, b: f64, alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, alpha)
}

pub fn is_dark(view: &NSView) -> bool {
    let names = unsafe { NSArray::from_slice(&[NSAppearanceNameDarkAqua, NSAppearanceNameAqua]) };
    view.effectiveAppearance()
        .bestMatchFromAppearancesWithNames(&names)
        .is_some_and(|name| &*name == unsafe { NSAppearanceNameDarkAqua })
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontWeight {
    Regular,
    Medium,
    Semibold,
}

// Match the CSS font stack instead of AppKit's heavier system control font.
pub fn font(size: f64, weight: FontWeight) -> Retained<NSFont> {
    FONTS.with_borrow_mut(|fonts| {
        fonts
            .entry(((size * 10.0).round() as u16, weight))
            .or_insert_with(|| {
                let names = match weight {
                    FontWeight::Regular => ["Inter-Regular", "SegoeUI", "Avenir-Book", "Helvetica"],
                    FontWeight::Medium => ["Inter-Medium", "SegoeUI", "Avenir-Medium", "Helvetica"],
                    FontWeight::Semibold => [
                        "Inter-SemiBold",
                        "SegoeUI-Semibold",
                        "Avenir-Heavy",
                        "Helvetica-Bold",
                    ],
                };
                names
                    .into_iter()
                    .find_map(|name| NSFont::fontWithName_size(&NSString::from_str(name), size))
                    .unwrap_or_else(|| NSFont::systemFontOfSize(size))
            })
            .clone()
    })
}

pub fn foreground(view: &NSView, muted: bool) -> Retained<NSColor> {
    if GLASS.get() {
        match (is_dark(view), muted) {
            (true, false) => color(244.0 / 255.0, 244.0 / 255.0, 250.0 / 255.0, 1.0),
            (true, true) => color(185.0 / 255.0, 185.0 / 255.0, 201.0 / 255.0, 1.0),
            (false, false) => color(32.0 / 255.0, 35.0 / 255.0, 46.0 / 255.0, 1.0),
            (false, true) => color(101.0 / 255.0, 103.0 / 255.0, 116.0 / 255.0, 1.0),
        }
    } else if muted {
        NSColor::secondaryLabelColor()
    } else {
        NSColor::labelColor()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum SurfaceKind {
    Control,
    Primary,
    Muted,
    Menu,
}

fn relief(view: &NSView, radius: f64, primary: bool, enabled: bool) {
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        layer.setMasksToBounds(false);
        if GLASS.get() && enabled {
            let dark = is_dark(view);
            let tint = if primary {
                color(25.0 / 255.0, 69.0 / 255.0, 168.0 / 255.0, 1.0)
            } else if dark {
                color(7.0 / 255.0, 7.0 / 255.0, 17.0 / 255.0, 1.0)
            } else {
                color(43.0 / 255.0, 45.0 / 255.0, 73.0 / 255.0, 1.0)
            };
            layer.setShadowColor(Some(&tint.CGColor()));
            layer.setShadowOpacity(if primary {
                0.18
            } else if dark {
                0.16
            } else {
                0.10
            });
            layer.setShadowOffset(NSSize::new(0.0, -2.0));
            layer.setShadowRadius(if primary { 4.0 } else { 3.0 });
            // Fixed geometry avoids deriving a shadow from the text/scroll contents.
            let path = unsafe {
                CGPath::with_rounded_rect(view.bounds(), radius, radius, std::ptr::null())
            };
            layer.setShadowPath(Some(&path));
        } else {
            layer.setShadowOpacity(0.0);
            layer.setShadowPath(None);
        }
        CATransaction::commit();
    }
}

fn linear(path: &NSBezierPath, colors: &[&NSColor], locations: &[f64], degrees: f64) {
    let Some(context) = NSGraphicsContext::currentContext() else {
        return;
    };
    let colors = NSArray::from_slice(colors);
    let gradient = unsafe {
        NSGradient::initWithColors_atLocations_colorSpace(
            NSGradient::alloc(),
            &colors,
            locations.as_ptr(),
            &NSColorSpace::sRGBColorSpace(),
        )
    };
    if let Some(gradient) = gradient {
        context.saveGraphicsState();
        path.addClip();
        // CSS measures angles clockwise from the top. These controls are flipped.
        let bounds = path.bounds();
        let dx = degrees.to_radians().sin();
        let dy = -degrees.to_radians().cos();
        let half = (bounds.size.width * dx.abs() + bounds.size.height * dy.abs()) / 2.0;
        let center = NSPoint::new(
            bounds.origin.x + bounds.size.width / 2.0,
            bounds.origin.y + bounds.size.height / 2.0,
        );
        gradient.drawFromPoint_toPoint_options(
            NSPoint::new(center.x - dx * half, center.y - dy * half),
            NSPoint::new(center.x + dx * half, center.y + dy * half),
            NSGradientDrawingOptions::DrawsBeforeStartingLocation
                | NSGradientDrawingOptions::DrawsAfterEndingLocation,
        );
        context.restoreGraphicsState();
    }
}

fn sheen(path: &NSBezierPath, tint: &NSColor, x: f64, end: f64) {
    let Some(context) = NSGraphicsContext::currentContext() else {
        return;
    };
    if let Some(gradient) = NSGradient::initWithStartingColor_endingColor(
        NSGradient::alloc(),
        tint,
        &tint.colorWithAlphaComponent(0.0),
    ) {
        context.saveGraphicsState();
        path.addClip();
        let bounds = path.bounds();
        let transform = NSAffineTransform::transform();
        transform.translateXBy_yBy(bounds.origin.x + bounds.size.width * x, bounds.origin.y);
        transform.scaleXBy_yBy(
            bounds.size.width * (1.0 - x) * end * std::f64::consts::SQRT_2,
            bounds.size.height * end * std::f64::consts::SQRT_2,
        );
        transform.concat();
        gradient.drawFromCenter_radius_toCenter_radius_options(
            NSPoint::new(0.0, 0.0),
            0.0,
            NSPoint::new(0.0, 0.0),
            1.0,
            NSGradientDrawingOptions::DrawsAfterEndingLocation,
        );
        context.restoreGraphicsState();
    }
}

// The native controls use the same tint, sheen and hover values as styles.css.
// Paint only inside the rounded surface, so recycled table rows have no shadow
// or opaque backing extending into the transparent space between entries.
pub fn paint_surface(
    view: &NSView,
    radius: f64,
    kind: SurfaceKind,
    hover: bool,
    pressed: bool,
    focused: bool,
) {
    let bounds = view.bounds();
    let glass = GLASS.get();
    let dark = is_dark(view);
    let shape = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
        rect(0.5, 0.5, bounds.size.width - 1.0, bounds.size.height - 1.0),
        radius,
        radius,
    );
    let context = NSGraphicsContext::currentContext();
    if let Some(context) = &context {
        context.saveGraphicsState();
    }
    let no_shadow = NSShadow::new();
    no_shadow.setShadowColor(None);
    no_shadow.set();
    if kind == SurfaceKind::Primary {
        let brightness = if pressed {
            0.95
        } else if hover {
            1.08
        } else {
            1.0
        };
        let blue = |r: f64, g: f64, b: f64, a| {
            color(
                (r * brightness).min(1.0),
                (g * brightness).min(1.0),
                (b * brightness).min(1.0),
                a,
            )
        };
        if glass {
            let tint = if dark {
                color(86.0 / 255.0, 86.0 / 255.0, 112.0 / 255.0, 0.42)
            } else {
                color(1.0, 1.0, 1.0, 0.43)
            };
            tint.setFill();
            shape.fill();
            linear(
                &shape,
                &[
                    &blue(56.0 / 255.0, 134.0 / 255.0, 252.0 / 255.0, 0.88),
                    &blue(38.0 / 255.0, 99.0 / 255.0, 224.0 / 255.0, 0.78),
                ],
                &[0.0, 1.0],
                160.0,
            );
            sheen(
                &shape,
                &blue(192.0 / 255.0, 220.0 / 255.0, 1.0, 0.45),
                0.20,
                0.65,
            );
        } else {
            blue(57.0 / 255.0, 108.0 / 255.0, 216.0 / 255.0, 1.0).setFill();
            shape.fill();
        }
    } else if glass {
        let fill = if kind == SurfaceKind::Menu {
            if dark {
                color(68.0 / 255.0, 68.0 / 255.0, 91.0 / 255.0, 0.84)
            } else {
                color(248.0 / 255.0, 249.0 / 255.0, 1.0, 0.82)
            }
        } else if dark {
            color(86.0 / 255.0, 86.0 / 255.0, 112.0 / 255.0, 0.42)
        } else {
            color(1.0, 1.0, 1.0, 0.43)
        };
        fill.setFill();
        shape.fill();
        if kind != SurfaceKind::Menu {
            linear(
                &shape,
                &if dark {
                    [
                        color(187.0 / 255.0, 190.0 / 255.0, 222.0 / 255.0, 0.20),
                        color(163.0 / 255.0, 166.0 / 255.0, 198.0 / 255.0, 0.11),
                        color(206.0 / 255.0, 209.0 / 255.0, 238.0 / 255.0, 0.18),
                    ]
                } else {
                    [
                        color(1.0, 1.0, 1.0, 0.40),
                        color(1.0, 1.0, 1.0, 0.12),
                        color(1.0, 1.0, 1.0, 0.28),
                    ]
                }
                .each_ref()
                .map(|c| &**c),
                &[0.0, 0.55, 1.0],
                160.0,
            );
            sheen(
                &shape,
                &*(if dark {
                    color(230.0 / 255.0, 233.0 / 255.0, 1.0, 0.14)
                } else {
                    color(1.0, 1.0, 1.0, 0.58)
                }),
                0.12,
                0.58,
            );
        }
        if hover {
            color(1.0, 1.0, 1.0, if dark { 0.09 } else { 0.16 }).setFill();
            shape.fill();
        }
        if pressed {
            color(0.0, 0.0, 0.0, 0.08).setFill();
            shape.fill();
        }
    } else {
        let fill = if kind == SurfaceKind::Muted {
            color(0.5, 0.5, 0.5, if hover { 0.16 } else { 0.08 })
        } else if dark {
            color(42.0 / 255.0, 42.0 / 255.0, 42.0 / 255.0, 1.0)
        } else {
            NSColor::whiteColor()
        };
        fill.setFill();
        shape.fill();
    }
    if glass {
        let inner = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            rect(1.5, 1.5, bounds.size.width - 3.0, bounds.size.height - 3.0),
            (radius - 1.0).max(0.0),
            (radius - 1.0).max(0.0),
        );
        let rim = NSBezierPath::bezierPath();
        rim.appendBezierPath(&shape);
        rim.appendBezierPath(&inner);
        rim.setWindingRule(NSWindingRule::EvenOdd);
        let (top, bottom) = if kind == SurfaceKind::Primary {
            (
                color(1.0, 1.0, 1.0, 0.72),
                color(186.0 / 255.0, 213.0 / 255.0, 1.0, 0.36),
            )
        } else if dark {
            (
                color(237.0 / 255.0, 239.0 / 255.0, 1.0, 0.56),
                color(219.0 / 255.0, 221.0 / 255.0, 249.0 / 255.0, 0.24),
            )
        } else {
            (color(1.0, 1.0, 1.0, 0.90), color(1.0, 1.0, 1.0, 0.46))
        };
        linear(&rim, &[&top, &bottom], &[0.0, 1.0], 180.0);
        // Keep the soft relief inside the rounded edge, away from row gaps.
        let glow = NSShadow::new();
        glow.setShadowColor(Some(&color(
            1.0,
            1.0,
            1.0,
            if kind == SurfaceKind::Primary {
                0.30
            } else if dark {
                0.09
            } else {
                0.25
            },
        )));
        glow.setShadowBlurRadius(if kind == SurfaceKind::Primary {
            1.0
        } else {
            4.5
        });
        glow.set();
        color(
            1.0,
            1.0,
            1.0,
            if kind == SurfaceKind::Primary {
                0.18
            } else if dark {
                0.06
            } else {
                0.16
            },
        )
        .setStroke();
        inner.setLineWidth(0.8);
        inner.stroke();
        no_shadow.set();
        if focused {
            color(127.0 / 255.0, 172.0 / 255.0, 1.0, 0.56).setStroke();
            shape.setLineWidth(1.0);
            shape.stroke();
        }
    } else {
        let rim = if focused || (hover && kind != SurfaceKind::Muted) {
            color(57.0 / 255.0, 108.0 / 255.0, 216.0 / 255.0, 1.0)
        } else {
            NSColor::separatorColor()
        };
        rim.setStroke();
        shape.setLineWidth(1.0);
        shape.stroke();
    }
    if let Some(context) = &context {
        context.restoreGraphicsState();
    }
}

pub fn text(
    value: &str,
    bounds: NSRect,
    size: f64,
    foreground: &NSColor,
    weight: FontWeight,
    alignment: NSTextAlignment,
) {
    let paragraph = NSMutableParagraphStyle::new();
    paragraph.setAlignment(alignment);
    paragraph.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    let font = font(size, weight);
    let attributes = NSDictionary::<NSString, AnyObject>::from_slices(
        &[
            unsafe { NSFontAttributeName },
            unsafe { NSForegroundColorAttributeName },
            unsafe { NSParagraphStyleAttributeName },
        ],
        &[font.as_ref(), foreground.as_ref(), paragraph.as_ref()],
    );
    let value = NSString::from_str(value);
    let height = unsafe { value.sizeWithAttributes(Some(&attributes)) }.height;
    let context = NSGraphicsContext::currentContext();
    if let Some(context) = &context {
        context.saveGraphicsState();
        // Match the reference's -webkit-font-smoothing: antialiased.
        CGContext::set_should_smooth_fonts(Some(&context.CGContext()), false);
    }
    let shadow = NSShadow::new();
    shadow.setShadowColor(None);
    shadow.set();
    unsafe {
        value.drawInRect_withAttributes(
            rect(
                bounds.origin.x,
                bounds.origin.y + (bounds.size.height - height) / 2.0,
                bounds.size.width,
                height,
            ),
            Some(&attributes),
        );
    }
    if let Some(context) = &context {
        context.restoreGraphicsState();
    }
}

#[derive(Default)]
pub struct ViewVars {
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
    pub hover_focus: Cell<bool>,
    surface: Cell<Option<(f64, SurfaceKind)>>,
    pub focused: Cell<bool>,
    pub active: Cell<bool>,
    pub checked: Cell<bool>,
    pub flipped: Cell<bool>,
    pub field_wrapper: Cell<bool>,
}

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ViewVars]
    pub struct View;

    unsafe impl NSObjectProtocol for View {}

    impl View {
        #[unsafe(method(isFlipped))]
        fn flipped(&self) -> bool { self.ivars().flipped.get() }

        #[unsafe(method(isOpaque))]
        fn opaque(&self) -> bool {false}

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self,event:&NSEvent) {
            if self.ivars().field_wrapper.get() {
                if let Some(field)=self.subviews().into_iter().find_map(|view|view.downcast::<NSTextField>().ok()) {
                    if let Some(window)=self.window(){window.makeFirstResponder(Some(&field));}
                    return;
                }
            }
            unsafe{let _:()=msg_send![super(self),mouseDown:event];}
        }

        #[unsafe(method(drawRect:))]
        fn draw(&self, _dirty: NSRect) {
            if let Some((radius, kind)) = self.ivars().surface.get() {
                paint_surface(self, radius, kind, false, false, self.ivars().focused.get());
            }
            if self.ivars().active.get() {
                color(57.0/255.0,108.0/255.0,216.0/255.0,1.0).setFill();
                let radius = if GLASS.get() {12.0} else {6.0};
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(), radius, radius).fill();
            }
            if self.ivars().checked.get() {
                let ink = if self.ivars().active.get() {NSColor::whiteColor()} else {NSColor::labelColor()};
                ink.setStroke();
                let path = NSBezierPath::bezierPath();
                let y = self.bounds().size.height / 2.0;
                path.moveToPoint(NSPoint::new(6.0,y));
                path.lineToPoint(NSPoint::new(9.0,y+2.5));
                path.lineToPoint(NSPoint::new(14.0,y-2.5));
                path.setLineWidth(1.5);path.setLineCapStyle(NSLineCapStyle::Round);path.setLineJoinStyle(NSLineJoinStyle::Round);path.stroke();
            }
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking(&self) {
            if let Some(area) = self.ivars().tracking.borrow_mut().take() { self.removeTrackingArea(&area); }
            unsafe { let _: () = msg_send![super(self), updateTrackingAreas]; }
            if self.ivars().hover_focus.get() {
                let area = unsafe { NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(), self.bounds(),
                    NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::MouseMoved | NSTrackingAreaOptions::ActiveAlways | NSTrackingAreaOptions::InVisibleRect,
                    Some(self), None,
                ) };
                self.addTrackingArea(&area);
                *self.ivars().tracking.borrow_mut() = Some(area);
            }
        }

        #[unsafe(method(mouseEntered:))]
        fn entered(&self, event: &NSEvent) { if self.ivars().hover_focus.get() { post(Action::Hover(true));post(Action::PointerMoved(event.locationInWindow())); } }

        #[unsafe(method(mouseExited:))]
        fn exited(&self, _event: &NSEvent) { if self.ivars().hover_focus.get() { post(Action::Hover(false)); } }

        #[unsafe(method(mouseMoved:))]
        fn moved(&self,event:&NSEvent) {
            if self.ivars().hover_focus.get() {
                post(Action::PointerMoved(event.locationInWindow()));
            }
        }
    }
);

impl View {
    pub fn new(bounds: NSRect, mtm: MainThreadMarker) -> Retained<Self> {
        let vars = ViewVars::default();
        vars.flipped.set(true);
        let this = Self::alloc(mtm).set_ivars(vars);
        unsafe { msg_send![super(this), initWithFrame: bounds] }
    }

    pub fn decorate(&self, radius: f64, kind: SurfaceKind) {
        self.ivars().surface.set(Some((radius, kind)));
        self.setWantsLayer(true);
        if let Some(layer) = self.layer() {
            layer.setBackgroundColor(None);
            layer.setBorderWidth(0.0);
            layer.setCornerRadius(radius);
        }
        relief(self, radius, kind == SurfaceKind::Primary, true);
        self.setNeedsDisplay(true);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Style {
    #[default]
    Secondary,
    Primary,
    Language,
    Close,
    Icon,
    Back,
    Row,
}

#[derive(Default)]
pub struct ButtonVars {
    pub style: Cell<Style>,
    pub hovered: Cell<bool>,
    pub hint: RefCell<String>,
    pub icon: Cell<u8>,
    pub muted: Cell<bool>,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    #[unsafe(super = NSButton)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ButtonVars]
    pub struct Button;

    unsafe impl NSObjectProtocol for Button {}

    impl Button {
        #[unsafe(method(isFlipped))]
        fn flipped(&self) -> bool { true }

        #[unsafe(method(isOpaque))]
        fn opaque(&self) -> bool {false}

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool { true }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking(&self) {
            if let Some(area) = self.ivars().tracking.borrow_mut().take() { self.removeTrackingArea(&area); }
            unsafe { let _: () = msg_send![super(self), updateTrackingAreas]; }
            let area = unsafe { NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(), self.bounds(),
                NSTrackingAreaOptions::MouseEnteredAndExited | NSTrackingAreaOptions::MouseMoved | NSTrackingAreaOptions::ActiveAlways | NSTrackingAreaOptions::InVisibleRect,
                Some(self), None,
            ) };
            self.addTrackingArea(&area);
            *self.ivars().tracking.borrow_mut() = Some(area);
        }

        #[unsafe(method(mouseEntered:))]
        fn entered(&self, event: &NSEvent) { self.update_hover(event); }

        #[unsafe(method(mouseExited:))]
        fn exited(&self, event: &NSEvent) { self.update_hover(event); }

        #[unsafe(method(mouseMoved:))]
        fn moved(&self,event:&NSEvent){self.update_hover(event);}

        #[unsafe(method(drawRect:))]
        fn draw(&self, _dirty: NSRect) {
            let bounds = self.bounds();
            let style = self.ivars().style.get();
            let glass = GLASS.get();
            let hover = self.ivars().hovered.get() && self.isEnabled();
            let width = bounds.size.width;
            let height = bounds.size.height;
            let radius = match style {
                Style::Close => height / 2.0,
                Style::Primary => if glass {22.0} else {10.0},
                Style::Icon | Style::Row => 6.0,
                Style::Back => if glass { height / 2.0 } else { 6.0 },
                _ => if glass { height / 2.0 } else {8.0},
            };
            let foreground = if style == Style::Primary || (style == Style::Icon && glass) { NSColor::whiteColor() } else { foreground(self,false) };
            if style == Style::Icon {
                let shape = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(),radius,radius);
                color(1.0, 1.0, 1.0, if hover { if glass {0.16} else {0.7} } else {0.0}).setFill();
                shape.fill();
            } else if style == Style::Row {
                let shape = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(),radius,radius);
                color(0.22, 0.43, 0.85, if hover {1.0} else {0.0}).setFill();
                shape.fill();
            } else if style == Style::Close {
                let shape = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(0.5,0.5,width-1.0,height-1.0),radius,radius);
                if glass && !hover {
                    paint_surface(self,radius,SurfaceKind::Control,false,self.isHighlighted(),false);
                } else {
                    color(0.5,0.5,0.5,if hover {if glass {0.44} else {0.30}} else {0.08}).setFill(); shape.fill();
                    if !hover {NSColor::separatorColor().setStroke();shape.setLineWidth(1.0);shape.stroke();}
                }
            } else {
                let kind = if style == Style::Primary {SurfaceKind::Primary}
                    else if self.ivars().muted.get() || style == Style::Back {SurfaceKind::Muted}
                    else {SurfaceKind::Control};
                paint_surface(self,radius,kind,hover,self.isHighlighted(),false);
                if hover && glass && style == Style::Language {
                    let shape = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(1.0,1.0,width-2.0,height-2.0),radius,radius);
                    color(1.0,1.0,1.0,0.21).setFill();shape.fill();
                }
            }
            let opacity = if self.isEnabled() {1.0} else {0.4};
            let foreground = foreground.colorWithAlphaComponent(opacity);
            if self.ivars().icon.get() == 7 {
                text("⇄",bounds,18.0,&foreground,FontWeight::Regular,NSTextAlignment::Center);
            } else if self.ivars().icon.get() > 0 {
                draw_icon(self.ivars().icon.get(), rect((width - 16.0) / 2.0, (height - 16.0) / 2.0, 16.0, 16.0), &foreground);
            } else if style == Style::Close {
                draw_icon(1, rect((width - 10.0) / 2.0, (height - 10.0) / 2.0, 10.0, 10.0), &foreground);
            } else {
                let title = self.title().to_string();
                let hint = self.ivars().hint.borrow();
                if hint.is_empty() {
                    let left = if style == Style::Language {14.0} else {6.0};
                    let align = if style == Style::Language {NSTextAlignment::Left} else {NSTextAlignment::Center};
                    let (size,weight)=match style {Style::Secondary=>(13.0,FontWeight::Medium),Style::Primary=>(15.0,FontWeight::Semibold),_=>(14.0,FontWeight::Regular)};
                    text(&title, rect(left, 0.0, width - left * 2.0 - if style == Style::Language {16.0} else {0.0}, height), size, &foreground, weight, align);
                } else {
                    let top=(height-35.0)/2.0+1.0;
                    text(&title, rect(4.0, top, width - 8.0, 18.0), 15.0, &foreground, FontWeight::Semibold, NSTextAlignment::Center);
                    text(&hint, rect(2.0, top + 21.0, width - 4.0, 14.0), 11.0, &foreground.colorWithAlphaComponent(0.72), FontWeight::Regular, NSTextAlignment::Center);
                }
                if style == Style::Language { draw_icon(8, rect(width - 23.0, (height - 12.0) / 2.0, 12.0, 12.0), &foreground.colorWithAlphaComponent(0.75)); }
            }
        }
    }
);

impl Button {
    pub fn set_hovered(&self, inside: bool) {
        if self.ivars().hovered.replace(inside) != inside {
            self.redraw();
        }
    }
    fn update_hover(&self, event: &NSEvent) {
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        let inside = self.mouse_inRect(point, self.bounds());
        // NSButton also owns a tracking area. Its enter/exit callbacks can
        // overlap ours, so derive the state from the pointer instead of letting
        // an unrelated exit cancel the highlight while still inside the button.
        self.set_hovered(inside);
    }
    pub fn redraw(&self) {
        let style = self.ivars().style.get();
        let height = self.bounds().size.height;
        let radius = match style {
            Style::Primary => {
                if GLASS.get() {
                    22.0
                } else {
                    10.0
                }
            }
            Style::Close => height / 2.0,
            _ => {
                if GLASS.get() {
                    height / 2.0
                } else {
                    8.0
                }
            }
        };
        relief(
            self,
            radius,
            style == Style::Primary,
            !matches!(style, Style::Icon | Style::Row | Style::Close),
        );
        NSView::setNeedsDisplay(self, true);
    }
    pub fn new(
        bounds: NSRect,
        title: &str,
        action: isize,
        style: Style,
        target: &AnyObject,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let vars = ButtonVars::default();
        vars.style.set(style);
        let this = Self::alloc(mtm).set_ivars(vars);
        let button: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: bounds] };
        button.setTitle(&NSString::from_str(title));
        button.setBordered(false);
        button.setButtonType(NSButtonType::MomentaryPushIn);
        button.setTag(action);
        unsafe {
            button.setTarget(Some(target));
            button.setAction(Some(objc2::sel!(clicked:)));
        }
        button
    }

    pub fn hint(&self, value: &str) {
        *self.ivars().hint.borrow_mut() = value.into();
        self.redraw();
    }
    pub fn icon(&self, value: u8) {
        self.ivars().icon.set(value);
        self.redraw();
    }
}

fn stroke(points: &[(f64, f64)], bounds: NSRect, path: &NSBezierPath) {
    let point = |(x, y): (f64, f64)| {
        NSPoint::new(
            bounds.origin.x + x * bounds.size.width / 24.0,
            bounds.origin.y + y * bounds.size.height / 24.0,
        )
    };
    if let Some(first) = points.first() {
        path.moveToPoint(point(*first));
        for value in &points[1..] {
            path.lineToPoint(point(*value));
        }
    }
}

pub fn draw_icon(kind: u8, bounds: NSRect, foreground: &NSColor) {
    let path = NSBezierPath::bezierPath();
    path.setLineWidth(if kind == 1 {
        bounds.size.width / 8.0
    } else {
        2.25 * bounds.size.width / 24.0
    });
    path.setLineCapStyle(NSLineCapStyle::Round);
    path.setLineJoinStyle(NSLineJoinStyle::Round);
    match kind {
        1 => {
            stroke(&[(4.5, 4.5), (19.5, 19.5)], bounds, &path);
            stroke(&[(19.5, 4.5), (4.5, 19.5)], bounds, &path);
        }
        2 => {
            stroke(
                &[
                    (8.0, 5.0),
                    (5.0, 5.0),
                    (4.0, 7.0),
                    (4.0, 21.0),
                    (13.0, 21.0),
                ],
                bounds,
                &path,
            );
            stroke(&[(16.0, 5.0), (20.0, 5.0), (20.0, 13.0)], bounds, &path);
            stroke(
                &[(8.0, 3.0), (16.0, 3.0), (16.0, 7.0), (8.0, 7.0), (8.0, 3.0)],
                bounds,
                &path,
            );
            stroke(&[(19.0, 16.0), (19.0, 23.0)], bounds, &path);
            stroke(&[(16.0, 19.0), (22.0, 19.0)], bounds, &path);
        }
        3 => {
            stroke(&[(4.0, 6.0), (20.0, 6.0)], bounds, &path);
            stroke(
                &[(9.0, 6.0), (9.0, 4.0), (15.0, 4.0), (15.0, 6.0)],
                bounds,
                &path,
            );
            stroke(
                &[(6.0, 6.0), (7.0, 21.0), (17.0, 21.0), (18.0, 6.0)],
                bounds,
                &path,
            );
            stroke(&[(10.0, 10.0), (10.0, 17.0)], bounds, &path);
            stroke(&[(14.0, 10.0), (14.0, 17.0)], bounds, &path);
        }
        4 => {
            stroke(
                &[
                    (9.0, 9.0),
                    (21.0, 9.0),
                    (21.0, 21.0),
                    (9.0, 21.0),
                    (9.0, 9.0),
                ],
                bounds,
                &path,
            );
            stroke(
                &[
                    (15.0, 9.0),
                    (15.0, 3.0),
                    (3.0, 3.0),
                    (3.0, 15.0),
                    (9.0, 15.0),
                ],
                bounds,
                &path,
            );
        }
        5 => {
            path.appendBezierPathWithOvalInRect(rect(
                bounds.origin.x + bounds.size.width * 0.15,
                bounds.origin.y + bounds.size.height * 0.12,
                bounds.size.width * 0.78,
                bounds.size.height * 0.78,
            ));
            stroke(&[(3.0, 3.0), (3.0, 9.0), (9.0, 9.0)], bounds, &path);
            stroke(&[(12.0, 7.0), (12.0, 13.0), (16.0, 15.0)], bounds, &path);
        }
        6 => {
            stroke(&[(19.0, 12.0), (5.0, 12.0), (12.0, 5.0)], bounds, &path);
            stroke(&[(5.0, 12.0), (12.0, 19.0)], bounds, &path);
        }
        7 => {
            stroke(&[(4.0, 8.0), (21.0, 8.0), (16.0, 3.0)], bounds, &path);
            stroke(&[(20.0, 16.0), (3.0, 16.0), (8.0, 21.0)], bounds, &path);
        }
        8 => stroke(&[(4.0, 8.0), (12.0, 16.0), (20.0, 8.0)], bounds, &path),
        9 => stroke(&[(4.0, 12.0), (10.0, 18.0), (21.0, 5.0)], bounds, &path),
        _ => {}
    }
    foreground.setStroke();
    path.stroke();
}

#[derive(Default)]
pub struct TextVars {
    pub placeholder: RefCell<String>,
    pub input: Cell<bool>,
}

define_class!(
    #[unsafe(super = NSTextView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TextVars]
    pub struct TextView;

    unsafe impl NSObjectProtocol for TextView {}

    impl TextView {
        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn appearance_changed(&self) {
            unsafe{let _:()=msg_send![super(self),viewDidChangeEffectiveAppearance];}
            self.setTextColor(Some(&foreground(self,false)));
            self.setNeedsDisplay(true);
        }

        #[unsafe(method(becomeFirstResponder))]
        fn become_first(&self)->bool {
            let accepted=unsafe{msg_send![super(self),becomeFirstResponder]};
            if accepted {self.focus_surface(true);}
            accepted
        }

        #[unsafe(method(resignFirstResponder))]
        fn resign_first(&self)->bool {
            let accepted=unsafe{msg_send![super(self),resignFirstResponder]};
            if accepted {self.focus_surface(false);}
            accepted
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self,event:&NSEvent) {
            post(Action::DismissPopup);
            unsafe{let _:()=msg_send![super(self),mouseDown:event];}
        }
        #[unsafe(method(didChangeText))]
        fn changed(&self) {
            unsafe { let _: () = msg_send![super(self), didChangeText]; }
            if self.ivars().input.get() { post(Action::InputEdited(self.string().to_string())); }
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if !NSTextInputClient::hasMarkedText(&**self) && event.modifierFlags().contains(NSEventModifierFlags::Command) && matches!(event.keyCode(), 36 | 76) {
                post(Action::Translate); return;
            }
            if !NSTextInputClient::hasMarkedText(&**self) && event.keyCode() == 53 { post(Action::Escape); return; }
            unsafe { let _: () = msg_send![super(self), keyDown: event]; }
        }

        #[unsafe(method(drawRect:))]
        fn draw(&self, dirty: NSRect) {
            let context=NSGraphicsContext::currentContext();
            if let Some(context)=&context {
                context.saveGraphicsState();
                CGContext::set_should_smooth_fonts(Some(&context.CGContext()),false);
            }
            unsafe { let _: () = msg_send![super(self), drawRect: dirty]; }
            if self.string().is_empty() {
                text(&self.ivars().placeholder.borrow(), rect(13.0,13.0,self.bounds().size.width-26.0,22.5), 15.0, &foreground(self,true), FontWeight::Regular, NSTextAlignment::Left);
            }
            if let Some(context)=&context {context.restoreGraphicsState();}
        }
    }
);

impl TextView {
    fn focus_surface(&self, focused: bool) {
        if let Some(surface) = self
            .enclosingScrollView()
            .and_then(|scroll| unsafe { scroll.superview() })
            .and_then(|view| view.downcast::<View>().ok())
        {
            surface.ivars().focused.set(focused);
            surface.setNeedsDisplay(true);
        }
    }

    pub fn new(bounds: NSRect, input: bool, mtm: MainThreadMarker) -> Retained<Self> {
        let vars = TextVars::default();
        vars.input.set(input);
        let this = Self::alloc(mtm).set_ivars(vars);
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: bounds] };
        view.setRichText(false);
        view.setEditable(input);
        view.setSelectable(true);
        view.setDrawsBackground(false);
        view.setFont(Some(&font(15.0, FontWeight::Regular)));
        view.setTextColor(Some(&NSColor::labelColor()));
        view.setTextContainerInset(NSSize::new(8.0, 12.0));
        let paragraph = NSMutableParagraphStyle::new();
        paragraph.setMinimumLineHeight(22.5);
        paragraph.setMaximumLineHeight(22.5);
        view.setDefaultParagraphStyle(Some(&paragraph));
        view.setVerticallyResizable(true);
        view.setHorizontallyResizable(false);
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        view.setMinSize(NSSize::new(0.0, bounds.size.height));
        view.setMaxSize(NSSize::new(f64::MAX, f64::MAX));
        view.setAutomaticQuoteSubstitutionEnabled(false);
        view.setAutomaticDashSubstitutionEnabled(false);
        view.setAutomaticSpellingCorrectionEnabled(false);
        view.setContinuousSpellCheckingEnabled(false);
        if let Some(container) = unsafe { view.textContainer() } {
            container.setWidthTracksTextView(true);
            container.setContainerSize(NSSize::new(bounds.size.width, f64::MAX));
        }
        view
    }
}

pub fn scroll(bounds: NSRect, mtm: MainThreadMarker) -> Retained<NSScrollView> {
    let view = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), bounds);
    view.setBorderType(NSBorderType::NoBorder);
    view.setDrawsBackground(false);
    view.contentView().setDrawsBackground(false);
    view.setHasVerticalScroller(true);
    view.setHasHorizontalScroller(false);
    view.setAutohidesScrollers(true);
    view.setScrollerStyle(NSScrollerStyle::Overlay);
    view
}

pub fn label(
    value: &str,
    bounds: NSRect,
    size: f64,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let view = NSTextField::labelWithString(&NSString::from_str(value), mtm);
    view.setFrame(bounds);
    view.setFont(Some(&font(size, FontWeight::Regular)));
    view.setTextColor(Some(&NSColor::labelColor()));
    view
}

pub fn surface(view: &NSView, radius: f64, primary: bool) {
    if let Some(view) = view.downcast_ref::<View>() {
        view.decorate(
            radius,
            if primary {
                SurfaceKind::Primary
            } else {
                SurfaceKind::Control
            },
        );
        return;
    }
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        let glass = GLASS.get();
        let fill = if primary {
            color(0.22, 0.43, 0.85, 1.0)
        } else if glass {
            if is_dark(view) {
                color(86.0 / 255.0, 86.0 / 255.0, 112.0 / 255.0, 0.42)
            } else {
                color(1.0, 1.0, 1.0, 0.43)
            }
        } else {
            NSColor::textBackgroundColor()
        };
        let border = if glass {
            color(1.0, 1.0, 1.0, if is_dark(view) { 0.40 } else { 0.68 })
        } else {
            NSColor::separatorColor()
        };
        layer.setCornerRadius(radius);
        layer.setMasksToBounds(true);
        layer.setBackgroundColor(Some(&fill.CGColor()));
        layer.setBorderColor(Some(&border.CGColor()));
        layer.setBorderWidth(1.0);
    }
}
