mod backend;
mod glass;
mod menu;
mod tables;
mod widgets;

// The same synchronous, event-driven material repair is used by both frontends.
#[path = "../../src-tauri/src/glass_blur.rs"]
mod glass_blur;

use dispatch2::{DispatchQueue, DispatchTime};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    sel, AnyThread, DefinedClass, MainThreadOnly,
};
use objc2_app_kit::*;
use objc2_foundation::{
    ns_string, MainThreadMarker, NSData, NSDate, NSDateFormatter, NSDateFormatterStyle, NSIndexSet,
    NSLocale, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSPoint, NSRect,
    NSSize, NSString,
};
use serde::Deserialize;
use simplet_core::{
    config::{Appearance, Config, ConfigUpdate},
    history::{HistoryEntry, HistoryPage},
    models::ModelsError,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

use crate::state::{Catalog, Page, Session, TranslationTicket};
use backend::{Backend, Change};
use tables::{List, Row};
use widgets::{rect, Button, Style, TextView, View};

thread_local! { static UI: RefCell<Option<Ui>> = const { RefCell::new(None) }; }

pub enum Action {
    Clicked(isize),
    Translate,
    Escape,
    DismissPopup,
    Tray {
        right: bool,
        anchor: Option<NSRect>,
    },
    InputEdited(String),
    FieldEdited(isize, String),
    Hover(bool),
    PointerMoved(NSPoint),
    HoverWake(u64, u64),
    FocusLost,
    Hide(u64),
    TableActivate(isize, isize),
    TableArrow(isize, u16),
    ProviderSelected(isize),
    Models {
        version: u64,
        result: Result<Vec<String>, ModelsError>,
    },
    FetchModels(u64),
    Translated {
        ticket: TranslationTicket,
        result: Result<String, String>,
        saved: Option<Result<HistoryEntry, String>>,
    },
    History {
        version: u64,
        older: bool,
        result: Result<HistoryPage, String>,
    },
    HistoryScrolled,
    Saved {
        version: u64,
        key_revision: u64,
        full: bool,
        result: Result<Config, String>,
    },
    ClearFeedback(u64),
}

pub fn post(action: Action) {
    DispatchQueue::main().exec_async(move || {
        UI.with_borrow_mut(|ui| {
            if let Some(ui) = ui {
                ui.handle(action);
            }
        });
    });
}

fn after(duration: Duration, action: Action) {
    if let Ok(when) = DispatchTime::try_from(duration) {
        let _ = DispatchQueue::main().after(when, move || post(action));
    }
}

#[derive(Deserialize)]
struct Language {
    value: String,
    code: String,
}
#[derive(Deserialize)]
struct Strings {
    languages: Vec<Language>,
    locales: BTreeMap<String, BTreeMap<String, String>>,
}
fn strings() -> &'static Strings {
    static STRINGS: OnceLock<Strings> = OnceLock::new();
    STRINGS.get_or_init(|| {
        serde_json::from_str(include_str!("../resources/strings.json"))
            .expect("validated localization resources")
    })
}

define_class!(
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    pub struct Panel;
    unsafe impl NSObjectProtocol for Panel {}
    impl Panel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_key(&self) -> bool {true}
        #[unsafe(method(canBecomeMainWindow))]
        fn can_main(&self) -> bool {false}
        #[unsafe(method(_hasActiveAppearance))]
        fn active_appearance(&self) -> bool {
            if widgets::GLASS.get() {true} else {unsafe{msg_send![super(self),_hasActiveAppearance]}}
        }
        #[unsafe(method(performKeyEquivalent:))]
        fn key_equivalent(&self,event:&NSEvent)->bool {
            let composing = self.firstResponder().and_then(|value|value.downcast::<NSTextView>().ok()).is_some_and(|text|NSTextInputClient::hasMarkedText(&*text));
            if !composing && event.modifierFlags().contains(NSEventModifierFlags::Command) && matches!(event.keyCode(),36|76) {post(Action::Translate);true}
            else { unsafe{msg_send![super(self),performKeyEquivalent:event]} }
        }
        #[unsafe(method(cancelOperation:))]
        fn cancel(&self,_sender:Option<&AnyObject>){post(Action::Escape);}
    }
);

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    pub struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    impl Delegate {
        #[unsafe(method(clicked:))]
        fn clicked(&self,sender:&NSControl) {
            post(Action::Clicked(sender.tag()));
        }
        #[unsafe(method(trayClicked:))]
        fn tray_clicked(&self,sender:&NSStatusBarButton) {
            let event=NSApplication::sharedApplication(self.mtm()).currentEvent();
            let right=event.as_ref().is_some_and(|event|event.r#type()==NSEventType::RightMouseDown);
            let anchor=event.and_then(|event|event.window(self.mtm())).or_else(||sender.window()).map(|window|window.frame());
            post(Action::Tray{right,anchor});
        }
        #[unsafe(method(menuAction:))]
        fn menu_action(&self,sender:&NSMenuItem){post(Action::Clicked(sender.tag()));}
        #[unsafe(method(historyScrolled:))]
        fn scrolled(&self,_notification:&NSNotification){post(Action::HistoryScrolled);}
    }
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn launched(&self,_notification:&NSNotification){
            match Ui::new(self,self.mtm()) {
                Ok(ui)=>{UI.with_borrow_mut(|slot|*slot=Some(ui));post(Action::Clicked(90));}
                Err(error)=>{eprintln!("SimpleT: {error}");NSApplication::sharedApplication(self.mtm()).terminate(None);}
            }
        }
    }
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowShouldClose:))]
        fn should_close(&self,_window:&NSWindow)->bool{post(Action::Clicked(1));false}
        #[unsafe(method(windowDidResignKey:))]
        fn resigned(&self,_notification:&NSNotification){post(Action::FocusLost);}
    }
    unsafe impl NSControlTextEditingDelegate for Delegate {
        #[unsafe(method(controlTextDidChange:))]
        fn field_changed(&self,notification:&NSNotification){
            if let Some(field)=notification.object().and_then(|object|object.downcast::<NSTextField>().ok()) {post(Action::FieldEdited(field.tag(),field.stringValue().to_string()));}
        }
        #[unsafe(method(controlTextDidBeginEditing:))]
        fn began_editing(&self,notification:&NSNotification){
            field_focus(notification,true);
            let search=notification.object().and_then(|object|object.downcast::<NSTextField>().ok()).is_some_and(|field|field.tag()==103);
            if !search {post(Action::DismissPopup);}
        }
        #[unsafe(method(controlTextDidEndEditing:))]
        fn ended_editing(&self,notification:&NSNotification){field_focus(notification,false);}
    }
    unsafe impl NSTextFieldDelegate for Delegate {}
);

fn field_focus(notification: &NSNotification, focused: bool) {
    if let Some(field) = notification
        .object()
        .and_then(|value| value.downcast::<NSTextField>().ok())
    {
        if let Some(parent) =
            unsafe { field.superview() }.and_then(|view| view.downcast::<View>().ok())
        {
            parent.ivars().focused.set(focused);
            parent.setNeedsDisplay(true);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Popup {
    LanguageA,
    LanguageB,
    Locale,
    Appearance,
    Model,
}

struct PopupView {
    kind: Popup,
    root: Retained<View>,
    frost: Option<Retained<NSVisualEffectView>>,
    choices: Option<List>,
    providers: Option<List>,
    models: Option<List>,
    search: Option<Retained<NSTextField>>,
}

struct Ui {
    mtm: MainThreadMarker,
    delegate: Retained<Delegate>,
    backend: Arc<Backend>,
    session: Session,
    saved: Config,
    appearance: Appearance,
    panel: Retained<Panel>,
    card: Retained<View>,
    _root: Retained<View>,
    effect: glass::Glass,
    tray: Retained<NSStatusItem>,
    pages: [Retained<View>; 3],
    buttons: BTreeMap<isize, Retained<Button>>,
    input: Retained<TextView>,
    output: Retained<TextView>,
    input_scroll: Retained<NSScrollView>,
    output_scroll: Retained<NSScrollView>,
    input_surface: Retained<View>,
    output_surface: Retained<View>,
    url: Retained<NSTextField>,
    key: Retained<NSTextField>,
    url_surface: Retained<View>,
    key_surface: Retained<View>,
    labels: Vec<(Retained<NSTextField>, &'static str)>,
    status: Retained<NSTextField>,
    config_status: Retained<NSTextField>,
    model_status: Retained<NSTextField>,
    history_status: Retained<NSTextField>,
    history: List,
    history_entries: Vec<HistoryEntry>,
    history_more: bool,
    history_loading: bool,
    history_version: u64,
    catalog: Catalog,
    catalog_ready: bool,
    model_version: u64,
    popup: Option<PopupView>,
    key_changed: bool,
    key_revision: u64,
    save_version: u64,
    full_save_version: u64,
    saving: bool,
    hover_inside: bool,
    hover_version: u64,
    feedback_version: u64,
}

impl Ui {
    fn new(delegate: &Delegate, mtm: MainThreadMarker) -> Result<Self, String> {
        let backend = Backend::new()?;
        let saved = backend.snapshot();
        let session = Session::new(saved.lang_a.clone(), saved.lang_b.clone());
        let this = Panel::alloc(mtm).set_ivars(());
        let panel: Retained<Panel> = unsafe {
            msg_send![super(this),initWithContentRect:rect(0.0,0.0,756.0,483.0),styleMask:NSWindowStyleMask::NonactivatingPanel,backing:NSBackingStoreType::Buffered,defer:false]
        };
        unsafe {
            panel.setReleasedWhenClosed(false);
        }
        panel.setTitle(ns_string!("SimpleT"));
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(false);
        panel.setHidesOnDeactivate(false);
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(false);
        panel.setAcceptsMouseMovedEvents(true);
        panel.setLevel(3);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::MoveToActiveSpace,
        );
        panel.setDelegate(Some(ProtocolObject::from_ref(delegate)));
        let root = View::new(rect(0.0, 0.0, 756.0, 483.0), mtm);
        // The reveal mask uses ordinary AppKit layer coordinates. Keep this
        // host unflipped; the card and its controls still lay out from the top.
        root.ivars().flipped.set(false);
        root.ivars().hover_focus.set(true);
        let card = View::new(rect(6.0, 6.0, 744.0, 471.0), mtm);
        root.addSubview(&card);
        panel.setContentView(Some(&root));
        let pages = [
            View::new(card.bounds(), mtm),
            View::new(card.bounds(), mtm),
            View::new(card.bounds(), mtm),
        ];
        for page in &pages {
            card.addSubview(page);
        }
        let mut buttons = BTreeMap::new();
        let mut add = |parent: &NSView, id, title, bounds, style, icon| {
            let button = Button::new(bounds, title, id, style, delegate, mtm);
            if icon > 0 {
                button.icon(icon);
            }
            parent.addSubview(&button);
            buttons.insert(id, button);
        };
        add(&card, 1, "", rect(725.0, 3.0, 16.0, 16.0), Style::Close, 0);
        let pane_width = 300.0;
        add(
            &pages[0],
            10,
            "",
            rect(16.0, 16.0, pane_width, 36.0),
            Style::Language,
            0,
        );
        add(
            &pages[0],
            11,
            "",
            rect(428.0, 16.0, pane_width, 36.0),
            Style::Language,
            0,
        );
        add(
            &pages[0],
            12,
            "",
            rect(352.0, 16.0, 40.0, 36.0),
            Style::Secondary,
            7,
        );
        add(
            &pages[0],
            20,
            "",
            rect(328.0, 185.0, 88.0, 56.0),
            Style::Primary,
            0,
        );
        add(
            &pages[0],
            21,
            "",
            rect(328.0, 253.0, 88.0, 36.0),
            Style::Secondary,
            0,
        );
        add(
            &pages[0],
            22,
            "",
            rect(328.0, 301.0, 88.0, 36.0),
            Style::Secondary,
            5,
        );
        add(
            &pages[0],
            30,
            "",
            rect(320.0, 68.0, 24.0, 24.0),
            Style::Icon,
            2,
        );
        add(
            &pages[0],
            31,
            "",
            rect(320.0, 96.0, 24.0, 24.0),
            Style::Icon,
            3,
        );
        add(
            &pages[0],
            32,
            "",
            rect(400.0, 68.0, 24.0, 24.0),
            Style::Icon,
            4,
        );
        add(
            &pages[0],
            33,
            "",
            rect(400.0, 96.0, 24.0, 24.0),
            Style::Icon,
            3,
        );
        add(
            &pages[1],
            40,
            "",
            rect(16.0, 20.0, 24.0, 24.0),
            Style::Back,
            6,
        );
        add(
            &pages[1],
            41,
            "",
            rect(586.0, 16.0, 142.0, 28.0),
            Style::Icon,
            0,
        );
        add(
            &pages[2],
            50,
            "",
            rect(16.0, 20.0, 24.0, 24.0),
            Style::Back,
            6,
        );
        add(
            &pages[2],
            51,
            "",
            rect(16.0, 232.0, 712.0, 36.0),
            Style::Language,
            0,
        );
        add(
            &pages[2],
            52,
            "",
            rect(16.0, 304.0, 712.0, 36.0),
            Style::Language,
            0,
        );
        add(
            &pages[2],
            53,
            "",
            rect(16.0, 376.0, 712.0, 36.0),
            Style::Language,
            0,
        );
        add(
            &pages[2],
            54,
            "",
            rect(16.0, 423.0, 88.0, 32.0),
            Style::Primary,
            0,
        );
        add(
            &pages[2],
            55,
            "",
            rect(640.0, 164.0, 88.0, 36.0),
            Style::Secondary,
            0,
        );
        add(
            &pages[2],
            56,
            "",
            rect(672.0, 116.0, 56.0, 20.0),
            Style::Icon,
            0,
        );
        let input_surface = View::new(rect(16.0, 64.0, pane_width, 391.0), mtm);
        let input_scroll = widgets::scroll(rect(0.0, 0.0, pane_width, 391.0), mtm);
        let input = TextView::new(rect(0.0, 0.0, pane_width, 391.0), true, mtm);
        input_scroll.setDocumentView(Some(&input));
        input_surface.addSubview(&input_scroll);
        pages[0].addSubview(&input_surface);
        let output_surface = View::new(rect(428.0, 64.0, pane_width, 391.0), mtm);
        let output_scroll = widgets::scroll(rect(0.0, 0.0, pane_width, 391.0), mtm);
        let output = TextView::new(rect(0.0, 0.0, pane_width, 391.0), false, mtm);
        output_scroll.setDocumentView(Some(&output));
        output_surface.addSubview(&output_scroll);
        pages[0].addSubview(&output_surface);
        let status = widgets::label("", rect(346.0, 130.0, 54.0, 45.0), 10.0, mtm);
        pages[0].addSubview(&status);
        let history = List::new(rect(16.0, 62.0, 712.0, 393.0), 4, mtm);
        pages[1].addSubview(&history.scroll);
        history
            .scroll
            .contentView()
            .setPostsBoundsChangedNotifications(true);
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                delegate,
                sel!(historyScrolled:),
                Some(NSViewBoundsDidChangeNotification),
                Some(&history.scroll.contentView()),
            );
        }
        let history_status = widgets::label("", rect(250.0, 22.0, 320.0, 20.0), 11.0, mtm);
        pages[1].addSubview(&history_status);
        let mut labels = Vec::new();
        for (parent, key, y) in [
            (&pages[1], "history", 18.0),
            (&pages[2], "settingsTitle", 18.0),
        ] {
            let label = widgets::label("", rect(48.0, y + 2.0, 500.0, 24.0), 18.0, mtm);
            label.setFont(Some(&widgets::font(18.0, widgets::FontWeight::Semibold)));
            parent.addSubview(&label);
            labels.push((label, key));
        }
        for (key, y) in [
            ("urlLabel", 58.0),
            ("keyLabel", 142.0),
            ("modelLabel", 210.0),
            ("uiLangLabel", 282.0),
            ("appearanceLabel", 354.0),
        ] {
            let label = widgets::label("", rect(16.0, y, 712.0, 19.0), 13.0, mtm);
            label.setTextColor(Some(&NSColor::secondaryLabelColor()));
            pages[2].addSubview(&label);
            labels.push((label, key));
        }
        let url_surface = View::new(rect(16.0, 80.0, 712.0, 34.0), mtm);
        let key_surface = View::new(rect(16.0, 164.0, 612.0, 36.0), mtm);
        url_surface.ivars().field_wrapper.set(true);
        key_surface.ivars().field_wrapper.set(true);
        pages[2].addSubview(&url_surface);
        pages[2].addSubview(&key_surface);
        let url = NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(11.0, 6.5, 690.0, 21.0));
        url.setTag(101);
        url.setStringValue(&NSString::from_str(&saved.base_url));
        url.setPlaceholderString(Some(ns_string!("https://api.openai.com/v1")));
        let key: Retained<NSTextField> = Retained::into_super(NSSecureTextField::initWithFrame(
            NSSecureTextField::alloc(mtm),
            rect(11.0, 7.5, 590.0, 21.0),
        ));
        key.setTag(102);
        for field in [&url, &key] {
            field.setFont(Some(&widgets::font(14.0, widgets::FontWeight::Regular)));
            field.setBezeled(false);
            field.setDrawsBackground(false);
            field.setFocusRingType(NSFocusRingType::None);
            unsafe {
                field.setDelegate(Some(ProtocolObject::from_ref(delegate)));
            }
        }
        url_surface.addSubview(&url);
        key_surface.addSubview(&key);
        let model_status = widgets::label("", rect(16.0, 116.0, 650.0, 20.0), 11.0, mtm);
        pages[2].addSubview(&model_status);
        let config_status = widgets::label("", rect(116.0, 428.0, 612.0, 24.0), 11.0, mtm);
        pages[2].addSubview(&config_status);
        let tray = NSStatusBar::systemStatusBar().statusItemWithLength(26.0);
        if let Some(button) = tray.button(mtm) {
            let data = NSData::with_bytes(include_bytes!("../../src-tauri/icons/tray.png"));
            if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
                image.setTemplate(true);
                image.setSize(NSSize::new(17.6, 17.6));
                button.setImage(Some(&image));
            }
            button.setToolTip(Some(ns_string!("SimpleT")));
            unsafe {
                button.setTarget(Some(delegate));
                button.setAction(Some(sel!(trayClicked:)));
                button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseDown);
            }
        }
        let mut ui = Self {
            mtm,
            delegate: unsafe {
                Retained::retain(delegate as *const Delegate as *mut Delegate).unwrap()
            },
            backend,
            session,
            saved: saved.clone(),
            appearance: saved.appearance,
            panel,
            card: card.clone(),
            _root: root.clone(),
            effect: glass::Glass::new(
                Retained::into_super(card),
                Retained::into_super(root.clone()),
            ),
            tray,
            pages,
            buttons,
            input,
            output,
            input_scroll,
            output_scroll,
            input_surface,
            output_surface,
            url,
            key,
            url_surface,
            key_surface,
            labels,
            status,
            config_status,
            model_status,
            history_status,
            history,
            history_entries: Vec::new(),
            history_more: false,
            history_loading: false,
            history_version: 0,
            catalog: Catalog::default(),
            catalog_ready: false,
            model_version: 0,
            popup: None,
            key_changed: false,
            key_revision: 0,
            save_version: 0,
            full_save_version: 0,
            saving: false,
            hover_inside: false,
            hover_version: 0,
            feedback_version: 0,
        };
        ui.catalog.selected = saved.model.clone();
        ui.buttons[&22].ivars().muted.set(true);
        ui.effect.set(ui.appearance == Appearance::LiquidGlass, mtm);
        ui.localize();
        ui.page(Page::Translate);
        ui.style();
        if let Some(error) = ui.backend.config.lock().unwrap().load_error.as_deref() {
            ui.config_status.setStringValue(&NSString::from_str(error));
        }
        Ok(ui)
    }

    fn t(&self, key: &str) -> String {
        strings()
            .locales
            .get(&self.saved.ui_lang)
            .or_else(|| strings().locales.get("zh"))
            .and_then(|dict| dict.get(key))
            .cloned()
            .unwrap_or_else(|| key.into())
    }

    fn language_name(&self, value: &str) -> String {
        let Some(language) = strings()
            .languages
            .iter()
            .find(|language| language.value == value)
        else {
            return value.into();
        };
        let locale = NSLocale::initWithLocaleIdentifier(
            NSLocale::alloc(),
            &NSString::from_str(&self.saved.ui_lang),
        );
        let display = locale
            .localizedStringForLanguageCode(&NSString::from_str(&language.code))
            .map(|name| name.to_string())
            .unwrap_or_else(|| value.into());
        let native = NSLocale::initWithLocaleIdentifier(
            NSLocale::alloc(),
            &NSString::from_str(&language.code),
        )
        .localizedStringForLanguageCode(&NSString::from_str(&language.code))
        .map(|name| name.to_string())
        .unwrap_or_default();
        if display.eq_ignore_ascii_case(&native) {
            display
        } else {
            format!("{display} ({native})")
        }
    }

    fn set_button(&self, id: isize, value: &str) {
        if let Some(button) = self.buttons.get(&id) {
            button.setTitle(&NSString::from_str(value));
            button.redraw();
        }
    }

    fn localize(&self) {
        for (label, key) in &self.labels {
            label.setStringValue(&NSString::from_str(&self.t(key)));
        }
        for (id, key) in [
            (20, "translate"),
            (21, "settings"),
            (41, "historyMore"),
            (54, "save"),
            (55, "clearKey"),
            (56, "retry"),
        ] {
            self.set_button(id, &self.t(key));
        }
        let hint = if self.session.busy {
            self.t("translating")
        } else {
            "⌘ + ↵".into()
        };
        self.buttons[&20].hint(&hint);
        self.set_button(10, &self.language_name(&self.session.lang_a));
        self.set_button(11, &self.language_name(&self.session.lang_b));
        self.set_button(
            51,
            &if self.catalog.selected.is_empty() {
                self.t("modelSelect")
            } else {
                self.catalog.selected.clone()
            },
        );
        let locale = strings()
            .locales
            .get(&self.saved.ui_lang)
            .and_then(|dict| dict.get("_name"))
            .cloned()
            .unwrap_or_else(|| self.saved.ui_lang.clone());
        self.set_button(52, &locale);
        self.set_button(
            53,
            &self.t(if self.appearance == Appearance::LiquidGlass {
                "appearanceGlass"
            } else {
                "appearanceDefault"
            }),
        );
        *self.input.ivars().placeholder.borrow_mut() = self.t("inputPh");
        *self.output.ivars().placeholder.borrow_mut() = self.t("outputPh");
        self.input.setNeedsDisplay(true);
        self.output.setNeedsDisplay(true);
        for (id, key) in [
            (1, "closeTitle"),
            (12, "swapTitle"),
            (22, "history"),
            (30, "pasteInput"),
            (31, "clearInput"),
            (32, "copyOutput"),
            (33, "clearOutput"),
        ] {
            self.buttons[&id].setToolTip(Some(&NSString::from_str(&self.t(key))));
        }
        self.refresh_actions();
    }

    fn refresh_actions(&self) {
        self.buttons[&20].setEnabled(!self.session.busy);
        self.buttons[&31].setEnabled(!self.session.input.is_empty());
        self.buttons[&32].setEnabled(!self.session.output.is_empty());
        self.buttons[&33].setEnabled(!self.session.output.is_empty());
        self.buttons[&51].setEnabled(self.catalog_ready);
        self.buttons[&54].setEnabled(!self.saving);
        self.key.setPlaceholderString(Some(&NSString::from_str(
            if !self.key_changed && !self.saved.api_key.is_empty() && self.same_endpoint() {
                "••••••••"
            } else {
                "sk-…"
            },
        )));
    }

    fn same_endpoint(&self) -> bool {
        let a = simplet_core::models::normalize_base_url(&self.url.stringValue().to_string());
        let b = simplet_core::models::normalize_base_url(&self.saved.base_url);
        a.is_ok() && a == b
    }

    fn style(&mut self) {
        let glass = self.appearance == Appearance::LiquidGlass;
        // The CSS reference also reserves the card's one-point border box.
        let padding = if glass { 15.0 } else { 17.0 };
        let middle = match self.saved.ui_lang.as_str() {
            "fr" | "ru" => 104.0,
            "de" => 112.0,
            _ => 88.0,
        };
        let width = (744.0 - padding * 2.0 - middle - 24.0) / 2.0;
        let center = padding + width + 12.0;
        let right = center + middle + 12.0;
        let top = padding + 48.0;
        let height = 471.0 - padding - top;
        self.buttons[&10].setFrame(rect(padding, padding, width, 36.0));
        self.buttons[&11].setFrame(rect(right, padding, width, 36.0));
        self.buttons[&12].setFrame(rect(center + (middle - 40.0) / 2.0, padding, 40.0, 36.0));
        self.input_surface
            .setFrame(rect(padding, top, width, height));
        self.output_surface
            .setFrame(rect(right, top, width, height));
        self.input_scroll.setFrame(rect(0.0, 0.0, width, height));
        self.output_scroll.setFrame(rect(0.0, 0.0, width, height));
        self.input.setFrameSize(NSSize::new(
            width,
            self.input.frame().size.height.max(height),
        ));
        self.output.setFrameSize(NSSize::new(
            width,
            self.output.frame().size.height.max(height),
        ));
        for (id, y, h) in [(20, 0.0, 56.0), (21, 68.0, 36.0), (22, 116.0, 36.0)] {
            self.buttons[&id].setFrame(rect(center, top + (height - 152.0) / 2.0 + y, middle, h));
        }
        for (id, x, y) in [
            (30, padding + width + 4.0, top + 4.0),
            (31, padding + width + 4.0, top + 32.0),
            (32, right - 28.0, top + 4.0),
            (33, right - 28.0, top + 32.0),
        ] {
            self.buttons[&id].setFrame(rect(x, y, 24.0, 24.0));
        }
        self.status.setFrame(rect(center, top + 68.0, middle, 42.0));
        self.status.setMaximumNumberOfLines(3);
        widgets::surface(&self.input_surface, if glass { 18.0 } else { 10.0 }, false);
        widgets::surface(&self.output_surface, if glass { 18.0 } else { 10.0 }, false);
        for scroll in [&self.input_scroll, &self.output_scroll] {
            scroll.setWantsLayer(true);
            if let Some(layer) = scroll.layer() {
                layer.setBackgroundColor(None);
                layer.setCornerRadius(if glass { 18.0 } else { 10.0 });
                layer.setMasksToBounds(true);
            }
        }
        for view in [&self.input, &self.output] {
            view.setTextColor(Some(&widgets::foreground(view, false)));
        }
        widgets::surface(&self.url_surface, if glass { 17.0 } else { 8.0 }, false);
        widgets::surface(&self.key_surface, if glass { 18.0 } else { 8.0 }, false);
        for button in self.buttons.values() {
            button.updateTrackingAreas();
            button.redraw();
        }
        self.surfaces();
    }

    fn surfaces(&mut self) {
        let mut surfaces = Vec::new();
        let mut add = |view: &NSView, radius: f64| {
            if !view.isHiddenOrHasHiddenAncestor() {
                let bounds = view.convertRect_toView(view.bounds(), Some(&self.card));
                surfaces.push(glass::Surface::new(bounds, radius));
            }
        };
        for (id, button) in &self.buttons {
            if !matches!(id, 30..=33 | 41 | 56) {
                add(
                    button,
                    if *id == 1 {
                        8.0
                    } else if *id == 20 || *id == 54 {
                        22.0
                    } else {
                        button.bounds().size.height / 2.0
                    },
                );
            }
        }
        add(&self.input_surface, 18.0);
        add(&self.output_surface, 18.0);
        add(&self.url_surface, 18.0);
        add(&self.key_surface, 18.0);
        if self.session.page == Page::History {
            let visible = self
                .history
                .table
                .rowsInRect(self.history.table.visibleRect());
            let viewport = self
                .history
                .scroll
                .convertRect_toView(self.history.scroll.bounds(), Some(&self.card));
            for row in visible.location..visible.location.saturating_add(visible.length) {
                let bounds = self.history.table.convertRect_toView(
                    self.history.table.frameOfCellAtColumn_row(0, row as isize),
                    Some(&self.card),
                );
                let top = bounds.origin.y.max(viewport.origin.y);
                let bottom = (bounds.origin.y + bounds.size.height)
                    .min(viewport.origin.y + viewport.size.height);
                if bottom > top {
                    surfaces.push(glass::Surface {
                        bounds,
                        clip: viewport,
                        radius: 20.0,
                    });
                }
            }
        }
        if let Some(popup) = &self.popup {
            surfaces.push(glass::Surface::new(popup.root.frame(), 18.0));
        }
        self.effect.surfaces(&surfaces);
    }

    fn page(&mut self, page: Page) {
        self.close_popup();
        if self.session.page == Page::History && page != Page::History {
            self.history_entries.clear();
            self.history.replace(Vec::new());
            self.history_version = self.history_version.wrapping_add(1);
            self.history_loading = false;
        }
        self.session.page = page;
        for (index, view) in self.pages.iter().enumerate() {
            view.setHidden(
                index
                    != match page {
                        Page::Translate => 0,
                        Page::History => 1,
                        Page::Settings => 2,
                    },
            );
        }
        self.focus_page();
        if page == Page::History {
            self.load_history(true);
        }
        self.surfaces();
    }

    fn focus_page(&self) {
        let responder: Option<&NSResponder> = match self.session.page {
            Page::Translate => Some(&self.input),
            Page::History => Some(&self.history.table),
            // Opening settings should not start editing or select its URL.
            Page::Settings => None,
        };
        self.panel.makeFirstResponder(responder);
    }

    fn show(&mut self, page: Page, anchor: Option<NSRect>) {
        self.session.show(page);
        self.page(page);
        let anchor = anchor.or_else(|| {
            self.tray
                .button(self.mtm)
                .and_then(|button| button.window())
                .map(|window| window.frame())
        });
        if let Some(anchor) = anchor {
            let middle = anchor.origin.x + anchor.size.width / 2.0;
            let screen = NSScreen::screens(self.mtm)
                .into_iter()
                .find(|screen| {
                    let frame = screen.frame();
                    middle >= frame.origin.x && middle < frame.origin.x + frame.size.width
                })
                .or_else(|| NSScreen::mainScreen(self.mtm));
            if let Some(screen) = screen {
                let frame = screen.visibleFrame();
                let x = (middle - 378.0)
                    .min(frame.origin.x + frame.size.width - 756.0)
                    .max(frame.origin.x);
                let top = (anchor.origin.y - 4.0)
                    .min(frame.origin.y + frame.size.height)
                    .max(frame.origin.y + 483.0);
                self.panel.setFrameTopLeftPoint(NSPoint::new(x, top));
            }
        } else {
            self.panel.center();
        }
        self.effect.reveal(false, false);
        self.panel.orderFrontRegardless();
        self.panel.makeKeyWindow();
        self.focus_page();
        self.effect.reveal(true, true);
        self._root.updateTrackingAreas();
    }

    fn hide(&mut self) {
        self.close_popup();
        self.hover_version = self.hover_version.wrapping_add(1);
        self.hover_inside = false;
        if let Some(generation) = self.session.request_hide() {
            self.effect.reveal(false, true);
            after(Duration::from_millis(320), Action::Hide(generation));
        }
    }

    fn close_popup(&mut self) {
        if let Some(popup) = self.popup.take() {
            popup.root.removeFromSuperview();
            if let Some(frost) = popup.frost {
                frost.removeFromSuperview();
            }
            self.surfaces();
        }
    }

    fn open_popup(&mut self, kind: Popup, button: isize) {
        if self.popup.as_ref().is_some_and(|popup| popup.kind == kind) {
            self.close_popup();
            return;
        }
        self.close_popup();
        let anchor = self.buttons[&button]
            .convertRect_toView(self.buttons[&button].bounds(), Some(&self.card));
        let options: Vec<(String, String, bool)> = match kind {
            Popup::LanguageA | Popup::LanguageB => strings()
                .languages
                .iter()
                .map(|language| {
                    (
                        language.value.clone(),
                        self.language_name(&language.value),
                        &language.value
                            == if kind == Popup::LanguageA {
                                &self.session.lang_a
                            } else {
                                &self.session.lang_b
                            },
                    )
                })
                .collect(),
            Popup::Locale => strings()
                .locales
                .iter()
                .map(|(code, dict)| {
                    (
                        code.clone(),
                        dict["_name"].clone(),
                        code == &self.saved.ui_lang,
                    )
                })
                .collect(),
            Popup::Appearance => vec![
                (
                    "default".into(),
                    self.t("appearanceDefault"),
                    self.appearance == Appearance::Default,
                ),
                (
                    "liquid_glass".into(),
                    self.t("appearanceGlass"),
                    self.appearance == Appearance::LiquidGlass,
                ),
            ],
            Popup::Model => Vec::new(),
        };
        let height = if kind == Popup::Model {
            180.0
        } else {
            options.len() as f64 * 28.0 + 12.0
        };
        let below = anchor.origin.y + anchor.size.height + 6.0;
        let y = if below + height < 465.0 {
            below
        } else {
            (anchor.origin.y - height - 6.0).max(6.0)
        };
        let height = height.min(465.0 - y);
        let root = View::new(
            rect(anchor.origin.x, y, anchor.size.width, height),
            self.mtm,
        );
        root.decorate(
            if widgets::GLASS.get() { 18.0 } else { 10.0 },
            widgets::SurfaceKind::Menu,
        );
        let frost = if widgets::GLASS.get() {
            // Menus must blur the controls underneath in this window as well
            // as the desktop material; the window's backdrop cannot do that.
            let frost = NSVisualEffectView::initWithFrame(
                NSVisualEffectView::alloc(self.mtm),
                root.frame(),
            );
            frost.setMaterial(NSVisualEffectMaterial::Popover);
            frost.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
            frost.setState(NSVisualEffectState::Active);
            frost.setWantsLayer(true);
            if let Some(layer) = frost.layer() {
                layer.setCornerRadius(18.0);
                layer.setMasksToBounds(true);
            }
            self.card.addSubview(&frost);
            Some(frost)
        } else {
            None
        };
        self.card.addSubview(&root);
        let mut popup = PopupView {
            kind,
            root,
            frost,
            choices: None,
            providers: None,
            models: None,
            search: None,
        };
        if kind == Popup::Model {
            let search_surface =
                View::new(rect(8.0, 8.0, anchor.size.width - 16.0, 28.0), self.mtm);
            search_surface.ivars().field_wrapper.set(true);
            widgets::surface(
                &search_surface,
                if widgets::GLASS.get() { 14.0 } else { 6.0 },
                false,
            );
            let search = NSTextField::initWithFrame(
                NSTextField::alloc(self.mtm),
                rect(8.0, 4.0, anchor.size.width - 32.0, 20.0),
            );
            search.setFont(Some(&widgets::font(13.0, widgets::FontWeight::Regular)));
            search.setBezeled(false);
            search.setDrawsBackground(false);
            search.setFocusRingType(NSFocusRingType::None);
            search.setTag(103);
            search.setPlaceholderString(Some(&NSString::from_str(&self.t("modelSearch"))));
            unsafe {
                search.setDelegate(Some(ProtocolObject::from_ref(&*self.delegate)));
            }
            search_surface.addSubview(&search);
            popup.root.addSubview(&search_surface);
            let split = (anchor.size.width * 0.3).max(120.0);
            let providers = List::new(rect(6.0, 44.0, split - 9.0, height - 50.0), 2, self.mtm);
            popup.root.addSubview(&providers.scroll);
            let models = List::new(
                rect(
                    split + 3.0,
                    44.0,
                    anchor.size.width - split - 9.0,
                    height - 50.0,
                ),
                3,
                self.mtm,
            );
            popup.root.addSubview(&models.scroll);
            popup.providers = Some(providers);
            popup.models = Some(models);
            popup.search = Some(search);
            self.catalog.query.clear();
        } else {
            let list = List::new(
                rect(6.0, 6.0, anchor.size.width - 12.0, height - 12.0),
                1,
                self.mtm,
            );
            let checked = options.iter().position(|option| option.2);
            list.replace(
                options
                    .into_iter()
                    .map(|(value, title, checked)| Row::choice(value, title, checked))
                    .collect(),
            );
            popup.root.addSubview(&list.scroll);
            if let Some(index) = checked {
                list.table.selectRowIndexes_byExtendingSelection(
                    &NSIndexSet::indexSetWithIndex(index),
                    false,
                );
                list.table.scrollRowToVisible(index as isize);
            }
            popup.choices = Some(list);
        }
        self.popup = Some(popup);
        if kind == Popup::Model {
            self.render_catalog();
        }
        if let Some(popup) = &self.popup {
            if let Some(search) = &popup.search {
                self.panel.makeFirstResponder(Some(search));
            } else if let Some(list) = &popup.choices {
                self.panel.makeFirstResponder(Some(&list.table));
            }
        }
        self.buttons[&1].removeFromSuperview();
        self.card.addSubview(&self.buttons[&1]);
        self.surfaces();
    }

    fn provider_label(&self, value: &str) -> String {
        match value {
            "" => self.t("modelCurrentService"),
            "openai" => "OpenAI".into(),
            "anthropic" => "Anthropic".into(),
            "google" => "Google".into(),
            "meta-llama" => "Meta".into(),
            _ => value.into(),
        }
    }

    fn render_catalog(&mut self) {
        let providers = self.catalog.providers();
        if !providers.contains(&self.catalog.provider) {
            self.catalog.provider = providers.first().cloned().unwrap_or_default();
        }
        if let Some(popup) = &self.popup {
            if let Some(list) = &popup.providers {
                list.replace(
                    providers
                        .iter()
                        .map(|value| Row::choice(value.clone(), self.provider_label(value), false))
                        .collect(),
                );
                if let Some(index) = providers
                    .iter()
                    .position(|value| value == &self.catalog.provider)
                {
                    list.table.selectRowIndexes_byExtendingSelection(
                        &NSIndexSet::indexSetWithIndex(index),
                        false,
                    );
                    list.table.scrollRowToVisible(index as isize);
                }
            }
            self.render_models();
        }
    }

    fn render_models(&self) {
        if let Some(list) = self.popup.as_ref().and_then(|popup| popup.models.as_ref()) {
            let models = self.catalog.models(&self.catalog.provider);
            let selected = models
                .iter()
                .position(|model| model == &self.catalog.selected);
            list.replace(
                models
                    .into_iter()
                    .map(|value| {
                        let title = value
                            .split_once('/')
                            .map_or(value.as_str(), |pair| pair.1)
                            .to_owned();
                        Row::choice(value.clone(), title, value == self.catalog.selected)
                    })
                    .collect(),
            );
            if let Some(index) = selected {
                list.table.selectRowIndexes_byExtendingSelection(
                    &NSIndexSet::indexSetWithIndex(index),
                    false,
                );
                list.table.scrollRowToVisible(index as isize);
            } else {
                unsafe {
                    list.table.deselectAll(None);
                }
            }
        }
    }

    fn choose(&mut self, value: String) {
        let Some(kind) = self.popup.as_ref().map(|popup| popup.kind) else {
            return;
        };
        self.close_popup();
        match kind {
            Popup::LanguageA => {
                self.session.lang_a = value;
                self.save_auto(Change::Languages(
                    self.session.lang_a.clone(),
                    self.session.lang_b.clone(),
                ));
            }
            Popup::LanguageB => {
                self.session.lang_b = value;
                self.save_auto(Change::Languages(
                    self.session.lang_a.clone(),
                    self.session.lang_b.clone(),
                ));
            }
            Popup::Locale => {
                self.saved.ui_lang = value.clone();
                self.save_auto(Change::Locale(value));
                self.style();
                self.render_history(false);
            }
            Popup::Appearance => {
                self.appearance = if value == "liquid_glass" {
                    Appearance::LiquidGlass
                } else {
                    Appearance::Default
                };
                self.effect
                    .set(self.appearance == Appearance::LiquidGlass, self.mtm);
                self.effect.reveal(true, false);
                self.style();
                self.render_history(false);
                self.save_auto(Change::Appearance(self.appearance));
            }
            Popup::Model => self.catalog.selected = value,
        }
        self.localize();
    }

    fn save_auto(&mut self, change: Change) {
        self.save_version = self.save_version.wrapping_add(1);
        self.backend
            .save(change, self.save_version, self.key_revision);
    }

    fn refresh_models(&mut self, delay: u64) {
        self.model_version = self.model_version.wrapping_add(1);
        self.catalog_ready = false;
        self.model_status
            .setStringValue(&NSString::from_str(&self.t("modelsLoading")));
        self.buttons[&56].setHidden(true);
        self.refresh_actions();
        after(
            Duration::from_millis(delay),
            Action::FetchModels(self.model_version),
        );
    }

    fn load_history(&mut self, reset: bool) {
        if self.history_loading {
            return;
        }
        if reset {
            self.history_version = self.history_version.wrapping_add(1);
            self.history_entries.clear();
            self.history.replace(Vec::new());
            self.history_more = false;
        }
        if !reset && !self.history_more {
            return;
        }
        self.history_loading = true;
        self.buttons[&41].setEnabled(false);
        self.history_status
            .setStringValue(&NSString::from_str(&self.t("historyLoading")));
        self.backend.history(
            self.history_version,
            if reset {
                None
            } else {
                self.history_entries.first().map(|entry| entry.id)
            },
        );
    }

    fn render_history(&mut self, bottom: bool) {
        let width = self.history.table.bounds().size.width - 12.0;
        let column = (width - 44.0) / 2.0;
        let formatter = NSDateFormatter::new();
        formatter.setLocale(Some(&NSLocale::initWithLocaleIdentifier(
            NSLocale::alloc(),
            &NSString::from_str(&self.saved.ui_lang),
        )));
        formatter.setDateStyle(NSDateFormatterStyle::MediumStyle);
        formatter.setTimeStyle(NSDateFormatterStyle::ShortStyle);
        let rows = self
            .history_entries
            .iter()
            .map(|entry| {
                let lines = |text: &str| -> f64 {
                    text.lines()
                        .map(|line| {
                            (line
                                .chars()
                                .map(|character| if character.is_ascii() { 7.5 } else { 15.0 })
                                .sum::<f64>()
                                / column)
                                .ceil()
                                .max(1.0)
                        })
                        .sum::<f64>()
                        .max(1.0)
                };
                let height = (lines(&entry.source_text).max(lines(&entry.translation)) * 22.5
                    + 64.0)
                    .max(112.0);
                Row {
                    key: entry.id.to_string(),
                    title: format!(
                        "{} → {}",
                        self.language_name(&entry.lang_a),
                        self.language_name(&entry.lang_b)
                    ),
                    trailing: formatter
                        .stringFromDate(&NSDate::dateWithTimeIntervalSince1970(
                            entry.created_at as f64 / 1000.0,
                        ))
                        .to_string(),
                    source: entry.source_text.clone(),
                    result: entry.translation.clone(),
                    height,
                    checked: false,
                }
            })
            .collect();
        self.history.replace(rows);
        if bottom && !self.history_entries.is_empty() {
            self.history
                .table
                .scrollRowToVisible(self.history_entries.len() as isize - 1);
        }
        self.buttons[&41].setHidden(!self.history_more);
        self.buttons[&41].setEnabled(!self.history_loading);
        self.surfaces();
    }

    fn tray_menu(&self, anchor: Option<NSRect>) {
        let menu = NSMenu::new(self.mtm);
        menu.setAutoenablesItems(false);
        for (id, title) in [
            (90, self.t("translate")),
            (91, self.t("settings")),
            (
                99,
                if self.saved.ui_lang == "zh" {
                    "退出".into()
                } else {
                    "Quit".into()
                },
            ),
        ] {
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(self.mtm),
                    &NSString::from_str(&title),
                    Some(sel!(menuAction:)),
                    ns_string!(""),
                )
            };
            item.setTag(id);
            unsafe {
                item.setTarget(Some(&self.delegate));
            }
            menu.addItem(&item);
        }
        let anchor = anchor.unwrap_or(rect(
            NSEvent::mouseLocation().x,
            NSEvent::mouseLocation().y,
            0.0,
            0.0,
        ));
        let point = NSPoint::new(
            anchor.origin.x + (anchor.size.width - menu.size().width) / 2.0,
            anchor.origin.y - 3.0,
        );
        menu.popUpMenuPositioningItem_atLocation_inView(None, point, None);
    }

    fn handle(&mut self, action: Action) {
        match action {
            Action::Clicked(id) => match id {
                1 => self.hide(),
                10 => self.open_popup(Popup::LanguageA, 10),
                11 => self.open_popup(Popup::LanguageB, 11),
                12 => {
                    self.close_popup();
                    self.session.swap();
                    self.input
                        .setString(&NSString::from_str(&self.session.input));
                    self.output
                        .setString(&NSString::from_str(&self.session.output));
                    self.feedback_version += 1;
                    self.save_auto(Change::Languages(
                        self.session.lang_a.clone(),
                        self.session.lang_b.clone(),
                    ));
                    self.localize();
                }
                20 => self.handle(Action::Translate),
                21 => self.page(Page::Settings),
                22 => self.page(Page::History),
                30 => {
                    self.close_popup();
                    if let Some(text) = NSPasteboard::generalPasteboard()
                        .stringForType(unsafe { NSPasteboardTypeString })
                    {
                        let text = self.session.input.clone() + &text.to_string();
                        self.session.edit_input(text);
                        self.input
                            .setString(&NSString::from_str(&self.session.input));
                        self.input.setSelectedRange(objc2_foundation::NSRange::new(
                            self.session.input.encode_utf16().count(),
                            0,
                        ));
                        self.input
                            .scrollRangeToVisible(NSTextInputClient::selectedRange(&**self.input));
                        self.panel.makeFirstResponder(Some(&self.input));
                        self.refresh_actions();
                    }
                }
                31 => {
                    self.close_popup();
                    self.session.edit_input(String::new());
                    self.input.setString(ns_string!(""));
                    self.panel.makeFirstResponder(Some(&self.input));
                    self.feedback_version += 1;
                    self.refresh_actions();
                }
                32 => {
                    self.close_popup();
                    if !self.session.output.is_empty() {
                        let clipboard = NSPasteboard::generalPasteboard();
                        clipboard.clearContents();
                        if clipboard
                            .setString_forType(&NSString::from_str(&self.session.output), unsafe {
                                NSPasteboardTypeString
                            })
                        {
                            self.feedback_version += 1;
                            self.buttons[&32].icon(9);
                            after(
                                Duration::from_millis(1500),
                                Action::ClearFeedback(self.feedback_version),
                            );
                        }
                    }
                }
                33 => {
                    self.close_popup();
                    self.session.clear_output();
                    self.output.setString(ns_string!(""));
                    self.feedback_version += 1;
                    self.buttons[&32].icon(4);
                    self.refresh_actions();
                }
                40 | 50 => self.page(Page::Translate),
                41 => self.load_history(!self.history_more),
                51 => {
                    if self.catalog_ready {
                        self.open_popup(Popup::Model, 51)
                    }
                }
                52 => self.open_popup(Popup::Locale, 52),
                53 => self.open_popup(Popup::Appearance, 53),
                54 => {
                    if self.saving {
                        return;
                    }
                    let base_url = self.url.stringValue().to_string().trim().to_owned();
                    if simplet_core::models::normalize_base_url(&base_url).is_err() {
                        self.config_status
                            .setStringValue(&NSString::from_str(&self.t("modelsInvalidUrl")));
                        return;
                    }
                    if self.catalog.selected.trim().is_empty() {
                        self.config_status
                            .setStringValue(&NSString::from_str(&self.t("modelSelect")));
                        return;
                    }
                    let config = ConfigUpdate {
                        base_url,
                        api_key: if self.key_changed {
                            Some(self.key.stringValue().to_string().trim().to_owned())
                        } else if self.same_endpoint() {
                            None
                        } else {
                            Some(String::new())
                        },
                        model: self.catalog.selected.clone(),
                        lang_a: self.session.lang_a.clone(),
                        lang_b: self.session.lang_b.clone(),
                        ui_lang: self.saved.ui_lang.clone(),
                        appearance: Some(self.appearance),
                    };
                    self.saving = true;
                    self.save_version += 1;
                    self.full_save_version = self.save_version;
                    self.backend
                        .save(Change::Full(config), self.save_version, self.key_revision);
                    self.refresh_actions();
                }
                55 => {
                    self.key.setStringValue(ns_string!(""));
                    self.key_changed = true;
                    self.key_revision += 1;
                    self.refresh_models(450);
                    self.refresh_actions();
                }
                56 => self.refresh_models(0),
                90 => {
                    self.show(Page::Translate, None);
                    if self.model_version == 0 {
                        self.refresh_models(0);
                    }
                }
                91 => {
                    self.show(Page::Settings, None);
                    if self.model_version == 0 {
                        self.refresh_models(0);
                    }
                }
                99 => NSApplication::sharedApplication(self.mtm).terminate(None),
                _ => {}
            },
            Action::Tray { right, anchor } => {
                if right {
                    self.tray_menu(anchor);
                } else if self.session.visible {
                    self.hide();
                } else {
                    self.show(Page::Translate, anchor);
                }
            }
            Action::Translate => {
                self.close_popup();
                self.session.edit_input(self.input.string().to_string());
                if let Some(ticket) = self.session.begin_translation() {
                    self.status.setStringValue(ns_string!(""));
                    self.backend.translate(ticket);
                } else if self.session.input.trim().is_empty() {
                    self.output.setString(ns_string!(""));
                }
                self.localize();
            }
            Action::Escape => {
                if self.popup.is_some() {
                    self.close_popup();
                } else {
                    self.hide();
                }
            }
            Action::DismissPopup => self.close_popup(),
            Action::InputEdited(value) => {
                self.session.edit_input(value);
                self.refresh_actions();
            }
            Action::FieldEdited(tag, value) => match tag {
                101 => {
                    self.close_popup();
                    self.refresh_models(450);
                    self.refresh_actions();
                }
                102 => {
                    self.key_changed = true;
                    self.key_revision += 1;
                    self.close_popup();
                    self.refresh_models(450);
                    self.refresh_actions();
                }
                103 => {
                    self.catalog.query = value;
                    self.render_catalog();
                }
                _ => {}
            },
            Action::Hover(inside) => {
                let entered = inside && !self.hover_inside;
                self.hover_inside = inside;
                if !inside {
                    for button in self.buttons.values() {
                        button.set_hovered(false);
                    }
                }
                self.hover_version += 1;
                if entered
                    && self.session.visible
                    && !self.session.closing
                    && !self.panel.isKeyWindow()
                {
                    after(
                        Duration::from_millis(120),
                        Action::HoverWake(self.hover_version, self.session.generation),
                    );
                }
            }
            Action::PointerMoved(point) => {
                if self.session.visible && !self.session.closing {
                    let covered = self.popup.as_ref().is_some_and(|popup| {
                        popup.root.mouse_inRect(
                            popup.root.convertPoint_fromView(point, None),
                            popup.root.bounds(),
                        )
                    });
                    for (id, button) in &self.buttons {
                        let inside = (!covered || *id == 1)
                            && !button.isHiddenOrHasHiddenAncestor()
                            && button.mouse_inRect(
                                button.convertPoint_fromView(point, None),
                                button.bounds(),
                            );
                        button.set_hovered(inside);
                    }
                }
            }
            Action::HoverWake(version, generation) => {
                if version == self.hover_version
                    && generation == self.session.generation
                    && self.hover_inside
                    && self.session.visible
                    && !self.session.closing
                    && !self.panel.isKeyWindow()
                {
                    self.panel.makeKeyWindow();
                    if self.panel.firstResponder().is_none() {
                        self.focus_page();
                    }
                }
            }
            Action::FocusLost => {
                self.hover_version += 1;
            }
            Action::Hide(generation) => {
                if self.session.commit_hide(generation) {
                    self.panel.orderOut(None);
                    glass_blur::suspend_hidden();
                }
            }
            Action::FetchModels(version) => {
                if version != self.model_version {
                    return;
                }
                if self.backend.fixture {
                    post(Action::Models {
                        version,
                        result: Ok((0..1000)
                            .map(|index| format!("vendor{}/model-{index}", index / 50))
                            .collect()),
                    });
                    return;
                }
                let url = self.url.stringValue().to_string();
                if simplet_core::models::normalize_base_url(&url).is_err() {
                    post(Action::Models {
                        version,
                        result: Err(ModelsError::InvalidUrl),
                    });
                    return;
                }
                self.backend.models(
                    version,
                    url,
                    if self.key_changed {
                        Some(self.key.stringValue().to_string())
                    } else {
                        None
                    },
                );
            }
            Action::Models { version, result } => {
                if version != self.model_version {
                    return;
                }
                match result {
                    Ok(models) => {
                        let count = models.len();
                        let candidate = if self.catalog.selected.is_empty() {
                            self.saved.model.clone()
                        } else {
                            self.catalog.selected.clone()
                        };
                        self.catalog.replace(models, &candidate);
                        self.catalog_ready = true;
                        self.model_status.setStringValue(&NSString::from_str(
                            &self.t("modelsReady").replace("{count}", &count.to_string()),
                        ));
                        self.buttons[&56].setHidden(true);
                    }
                    Err(error) => {
                        let key = match error {
                            ModelsError::InvalidUrl => "modelsInvalidUrl",
                            ModelsError::Http { status: 401 | 403 } => "modelsAuth",
                            ModelsError::Timeout => "modelsTimeout",
                            ModelsError::Network => "modelsNetwork",
                            ModelsError::Empty => "modelsEmpty",
                            _ => "modelsInvalidResponse",
                        };
                        self.model_status
                            .setStringValue(&NSString::from_str(&self.t(key)));
                        self.catalog_ready = false;
                        self.buttons[&56].setHidden(false);
                    }
                }
                self.localize();
            }
            Action::TableActivate(kind, index) => {
                let selected = self
                    .popup
                    .as_ref()
                    .and_then(|popup| match kind {
                        1 => popup.choices.as_ref(),
                        2 => popup.providers.as_ref(),
                        3 => popup.models.as_ref(),
                        _ => None,
                    })
                    .and_then(|list| list.key(index));
                if let Some(value) = selected {
                    if kind == 2 {
                        self.catalog.provider = value;
                        self.render_models();
                    } else {
                        self.choose(value);
                    }
                }
            }
            Action::ProviderSelected(index) => {
                if let Some(value) = self
                    .popup
                    .as_ref()
                    .and_then(|popup| popup.providers.as_ref())
                    .and_then(|list| list.key(index))
                {
                    if self.catalog.provider != value {
                        self.catalog.provider = value;
                        self.render_models();
                    }
                }
            }
            Action::TableArrow(kind, key) => {
                if let Some(popup) = &self.popup {
                    let target = if kind == 2 && key == 124 {
                        popup.models.as_ref()
                    } else if kind == 3 && key == 123 {
                        popup.providers.as_ref()
                    } else {
                        None
                    };
                    if let Some(list) = target {
                        self.panel.makeFirstResponder(Some(&list.table));
                        if list.table.selectedRow() < 0
                            && !list.source.ivars().rows.borrow().is_empty()
                        {
                            list.table.selectRowIndexes_byExtendingSelection(
                                &NSIndexSet::indexSetWithIndex(0),
                                false,
                            );
                        }
                    }
                }
            }
            Action::Translated {
                ticket,
                result,
                saved,
            } => {
                self.session.edit_input(self.input.string().to_string());
                let current = self.session.finish_translation(&ticket);
                let mut warning = None;
                if let Some(saved) = saved {
                    match saved {
                        Ok(entry) => {
                            if self.session.page == Page::History {
                                let bounds = self.history.scroll.contentView().bounds();
                                let bottom = bounds.origin.y + bounds.size.height
                                    >= self.history.table.bounds().size.height - 24.0;
                                if !self
                                    .history_entries
                                    .iter()
                                    .any(|value| value.id == entry.id)
                                {
                                    self.history_entries.push(entry);
                                    self.history_entries.sort_by_key(|entry| entry.id);
                                    self.render_history(bottom);
                                }
                                self.history_status.setStringValue(ns_string!(""));
                            }
                        }
                        Err(error) => {
                            warning = Some(error.clone());
                            if self.session.page == Page::History {
                                self.history_status
                                    .setStringValue(&NSString::from_str(&error));
                            }
                        }
                    }
                }
                if current {
                    match result {
                        Ok(translation) => {
                            self.session.output = translation;
                            self.status
                                .setStringValue(&NSString::from_str(&warning.unwrap_or_default()));
                        }
                        Err(error) => {
                            self.session.output.clear();
                            self.status.setStringValue(&NSString::from_str(&error));
                            self.status.setToolTip(Some(&NSString::from_str(&error)));
                        }
                    }
                    self.output
                        .setString(&NSString::from_str(&self.session.output));
                    self.feedback_version += 1;
                    self.buttons[&32].icon(4);
                }
                self.localize();
            }
            Action::History {
                version,
                older,
                result,
            } => {
                if version != self.history_version || self.session.page != Page::History {
                    return;
                }
                self.history_loading = false;
                match result {
                    Ok(page) => {
                        let old_height = self.history.table.bounds().size.height;
                        let old_y = self.history.scroll.contentView().bounds().origin.y;
                        let mut entries: BTreeMap<_, _> = self
                            .history_entries
                            .drain(..)
                            .map(|entry| (entry.id, entry))
                            .collect();
                        entries.extend(page.entries.into_iter().map(|entry| (entry.id, entry)));
                        self.history_entries = entries.into_values().collect();
                        self.history_more = page.has_more;
                        self.render_history(!older);
                        if older {
                            let new_height = self.history.table.bounds().size.height;
                            let clip = self.history.scroll.contentView();
                            clip.scrollToPoint(NSPoint::new(
                                0.0,
                                (old_y + new_height - old_height).max(0.0),
                            ));
                            self.history.scroll.reflectScrolledClipView(&clip);
                        }
                        let message = page.save_error.unwrap_or_else(|| {
                            if self.history_entries.is_empty() {
                                self.t("historyEmpty")
                            } else {
                                String::new()
                            }
                        });
                        self.history_status
                            .setStringValue(&NSString::from_str(&message));
                    }
                    Err(error) => {
                        self.history_status
                            .setStringValue(&NSString::from_str(&error));
                        self.buttons[&41].setHidden(false);
                        self.buttons[&41].setEnabled(true);
                        self.set_button(41, &self.t("retry"));
                    }
                }
                self.surfaces();
            }
            Action::HistoryScrolled => {
                if self.session.page == Page::History {
                    self.surfaces();
                    if self.history.scroll.contentView().bounds().origin.y < 60.0 {
                        self.load_history(false);
                    }
                }
            }
            Action::Saved {
                version,
                key_revision,
                full,
                result,
            } => {
                if full && version == self.full_save_version {
                    self.saving = false;
                }
                match result {
                    Ok(config) => {
                        let unchanged = self.url.stringValue().to_string().trim()
                            == config.base_url
                            && self.catalog.selected == config.model
                            && self.appearance == config.appearance
                            && self.key_revision == key_revision;
                        let locale = self.saved.ui_lang.clone();
                        self.saved = config;
                        self.saved.ui_lang = locale;
                        if full && self.key_revision == key_revision {
                            self.key_changed = false;
                            self.key.setStringValue(ns_string!(""));
                        }
                        if full {
                            self.config_status.setStringValue(&NSString::from_str(
                                &self.t(if unchanged { "saved" } else { "unsaved" }),
                            ));
                        }
                    }
                    Err(error) => {
                        if full || version == self.save_version {
                            self.config_status
                                .setStringValue(&NSString::from_str(&error));
                        }
                    }
                }
                self.refresh_actions();
            }
            Action::ClearFeedback(version) => {
                if version == self.feedback_version {
                    self.buttons[&32].icon(4);
                }
            }
        }
    }
}

pub fn run() {
    let mtm = MainThreadMarker::new().expect("AppKit runs on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // This standalone app owns its AppKit event loop and must be allowed to
    // present windows. The nonactivating panel keeps ordinary tray use from
    // activating the app; unlike Tauri, orderFrontRegardless does not activate it.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    menu::install(&app);
    let delegate = unsafe {
        let this = Delegate::alloc(mtm);
        msg_send![this, init]
    };
    let delegate: Retained<Delegate> = delegate;
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
