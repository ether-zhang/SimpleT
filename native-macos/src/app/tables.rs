use std::cell::{Cell, RefCell};

use objc2::{
    define_class, msg_send, rc::Retained, runtime::ProtocolObject, AnyThread, DefinedClass,
    MainThreadOnly,
};
use objc2_app_kit::*;
use objc2_foundation::{
    ns_string, MainThreadMarker, NSIndexSet, NSNotification, NSObject, NSObjectProtocol, NSRect,
    NSSize, NSString,
};

use super::{
    post,
    widgets::{self, rect},
    Action,
};

#[derive(Clone)]
pub struct Row {
    pub key: String,
    pub title: String,
    pub trailing: String,
    pub source: String,
    pub result: String,
    pub height: f64,
    pub checked: bool,
}

impl Row {
    pub fn choice(key: String, title: String, checked: bool) -> Self {
        Self {
            key,
            title,
            trailing: String::new(),
            source: String::new(),
            result: String::new(),
            height: 28.0,
            checked,
        }
    }
}

#[derive(Default)]
pub struct TableVars {
    pub kind: Cell<isize>,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    #[unsafe(super = NSTableView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TableVars]
    pub struct Table;
    unsafe impl NSObjectProtocol for Table {}
    impl Table {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self,_event:Option<&NSEvent>)->bool{true}
        #[unsafe(method(isOpaque))]
        fn opaque(&self) -> bool {false}

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking(&self) {
            if let Some(area)=self.ivars().tracking.borrow_mut().take(){self.removeTrackingArea(&area);}
            unsafe{let _:()=msg_send![super(self),updateTrackingAreas];}
            if self.ivars().kind.get()!=4 {
                let area=unsafe{NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),self.bounds(),
                    NSTrackingAreaOptions::MouseMoved|NSTrackingAreaOptions::MouseEnteredAndExited|NSTrackingAreaOptions::ActiveAlways|NSTrackingAreaOptions::InVisibleRect,
                    Some(self),None)};
                self.addTrackingArea(&area);*self.ivars().tracking.borrow_mut()=Some(area);
            }
        }

        #[unsafe(method(mouseMoved:))]
        fn moved(&self,event:&NSEvent){self.follow_pointer(event);}
        #[unsafe(method(mouseEntered:))]
        fn entered(&self,event:&NSEvent){self.follow_pointer(event);}

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            match event.keyCode() {
                36 | 76 => { post(Action::TableActivate(self.ivars().kind.get(), self.selectedRow())); return; }
                53 => { post(Action::Escape); return; }
                123 | 124 => { post(Action::TableArrow(self.ivars().kind.get(), event.keyCode())); return; }
                _ => {}
            }
            unsafe { let _: () = msg_send![super(self), keyDown: event]; }
        }
    }
);

impl Table {
    fn follow_pointer(&self, event: &NSEvent) {
        let row = self.rowAtPoint(self.convertPoint_fromView(event.locationInWindow(), None));
        if row >= 0 && row != self.selectedRow() {
            self.selectRowIndexes_byExtendingSelection(
                &NSIndexSet::indexSetWithIndex(row as usize),
                false,
            );
        }
    }
}

define_class!(
    #[unsafe(super = NSTableRowView)]
    #[thread_kind = MainThreadOnly]
    struct TransparentRow;
    unsafe impl NSObjectProtocol for TransparentRow {}
    impl TransparentRow {
        #[unsafe(method(isOpaque))]
        fn opaque(&self)->bool{false}
        #[unsafe(method(drawBackgroundInRect:))]
        fn background(&self,_dirty:NSRect){}
        #[unsafe(method(drawSelectionInRect:))]
        fn selection(&self,_dirty:NSRect){}
        #[unsafe(method(drawSeparatorInRect:))]
        fn separator(&self,_dirty:NSRect){}
    }
);

fn highlight_cell(cell: &NSView, active: bool) {
    if let Some(view) = cell.downcast_ref::<widgets::View>() {
        view.ivars().active.set(active);
        view.setNeedsDisplay(true);
    }
    for child in cell.subviews() {
        if let Ok(label) = child.downcast::<NSTextField>() {
            label.setTextColor(Some(
                &*(if active {
                    NSColor::whiteColor()
                } else {
                    NSColor::labelColor()
                }),
            ));
        }
    }
}

#[derive(Default)]
pub struct SourceVars {
    pub rows: RefCell<Vec<Row>>,
    pub kind: Cell<isize>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = SourceVars]
    pub struct Source;

    unsafe impl NSObjectProtocol for Source {}
    unsafe impl NSControlTextEditingDelegate for Source {}

    impl Source {
        #[unsafe(method(clicked:))]
        fn clicked(&self, table: &NSTableView) {
            post(Action::TableActivate(self.ivars().kind.get(), table.clickedRow()));
        }
    }

    unsafe impl NSTableViewDataSource for Source {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn count(&self, _table: &NSTableView) -> isize { self.ivars().rows.borrow().len() as isize }
    }

    unsafe impl NSTableViewDelegate for Source {
        #[unsafe(method_id(tableView:rowViewForRow:))]
        fn row_view(&self,table:&NSTableView,_index:isize)->Option<Retained<NSTableRowView>> {
            let view=unsafe{table.makeViewWithIdentifier_owner(ns_string!("transparent-row"),None)}
                .and_then(|view|view.downcast::<NSTableRowView>().ok()).unwrap_or_else(||{
                    let this=TransparentRow::alloc(self.mtm()).set_ivars(());
                    let view:Retained<TransparentRow>=unsafe{msg_send![super(this),initWithFrame:table.bounds()]};
                    view.setIdentifier(Some(ns_string!("transparent-row")));
                    Retained::into_super(view)
                });
            view.setBackgroundColor(&NSColor::clearColor());
            Some(view)
        }
        #[unsafe(method(tableView:heightOfRow:))]
        fn height(&self, _table: &NSTableView, row: isize) -> f64 {
            self.ivars().rows.borrow().get(row as usize).map_or(28.0, |row| row.height)
        }

        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn view(&self, table: &NSTableView, column: Option<&NSTableColumn>, index: isize) -> Option<Retained<NSView>> {
            (|| {
            let rows = self.ivars().rows.borrow();
            let row = rows.get(index as usize)?;
            let history = self.ivars().kind.get() == 4;
            let identifier = if history { ns_string!("history") } else { ns_string!("choice") };
            let width = column.map_or(table.bounds().size.width,|column|column.width());
            let cell = unsafe { table.makeViewWithIdentifier_owner(identifier, None) }.unwrap_or_else(|| {
                let cell = widgets::View::new(rect(0.0,0.0,width,row.height),self.mtm());
                cell.setIdentifier(Some(identifier));
                let fields = if history {4} else {1};
                for _ in 0..fields {
                    let label = if history { NSTextField::wrappingLabelWithString(ns_string!(""),self.mtm()) }
                        else { NSTextField::labelWithString(ns_string!(""),self.mtm()) };
                    label.setFont(Some(&widgets::font(14.0,widgets::FontWeight::Regular)));
                    label.setTextColor(Some(&NSColor::labelColor()));
                    label.setSelectable(history);
                    cell.addSubview(&label);
                }
                Retained::into_super(cell)
            });
            cell.setFrame(rect(0.0,0.0,width,row.height));
            let fields: Vec<_> = cell.subviews().into_iter().filter_map(|view| view.downcast::<NSTextField>().ok()).collect();
            if history {
                widgets::surface(&cell, if widgets::GLASS.get() {20.0} else {10.0}, false);
                fields[0].setFrame(rect(16.0,14.0,width*0.58-20.0,20.0));
                fields[0].setFont(Some(&widgets::font(11.0,widgets::FontWeight::Regular)));
                fields[0].setTextColor(Some(&NSColor::secondaryLabelColor()));
                fields[0].setStringValue(&NSString::from_str(&row.title));
                fields[1].setFrame(rect(width*0.58,14.0,width*0.42-16.0,20.0));
                fields[1].setFont(Some(&widgets::font(11.0,widgets::FontWeight::Regular)));
                fields[1].setAlignment(NSTextAlignment::Right);
                fields[1].setTextColor(Some(&NSColor::secondaryLabelColor()));
                fields[1].setStringValue(&NSString::from_str(&row.trailing));
                let column = (width-44.0)/2.0;
                fields[2].setFrame(rect(16.0,42.0,column,row.height-56.0));
                fields[2].setStringValue(&NSString::from_str(&row.source));
                fields[3].setFrame(rect(28.0+column,42.0,column,row.height-56.0));
                fields[3].setStringValue(&NSString::from_str(&row.result));
            } else {
                let label = &fields[0];
                let left=if self.ivars().kind.get()==2 {6.0} else {18.0};
                label.setFrame(rect(left,4.0,width-left-6.0,20.0));
                label.setStringValue(&NSString::from_str(&row.title));
                if let Some(view)=cell.downcast_ref::<widgets::View>(){view.ivars().checked.set(row.checked);}
                highlight_cell(&cell,table.selectedRow()==index);
                label.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
                cell.setToolTip(Some(&NSString::from_str(&row.key)));
            }
            Some(cell)
            })()
        }

        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, notification: &NSNotification) {
            if let Some(table) = notification.object().and_then(|object| object.downcast::<NSTableView>().ok()) {
                if self.ivars().kind.get()!=4 {
                    let visible=table.rowsInRect(table.visibleRect());
                    for row in visible.location..visible.location.saturating_add(visible.length) {
                        if let Some(cell)=table.viewAtColumn_row_makeIfNecessary(0,row as isize,false) {
                            highlight_cell(&cell,table.selectedRow()==row as isize);
                        }
                    }
                }
                if self.ivars().kind.get()==2 {post(Action::ProviderSelected(table.selectedRow()));}
            }
        }
    }
);

pub struct List {
    pub scroll: Retained<NSScrollView>,
    pub table: Retained<Table>,
    pub source: Retained<Source>,
}

impl List {
    pub fn new(bounds: objc2_foundation::NSRect, kind: isize, mtm: MainThreadMarker) -> Self {
        let vars = TableVars::default();
        vars.kind.set(kind);
        let this = Table::alloc(mtm).set_ivars(vars);
        let table: Retained<Table> = unsafe {
            msg_send![super(this), initWithFrame: rect(0.0,0.0,bounds.size.width,bounds.size.height)]
        };
        table.setHeaderView(None);
        table.setRowHeight(28.0);
        table.setIntercellSpacing(NSSize::new(0.0, if kind == 4 { 12.0 } else { 0.0 }));
        table.setBackgroundColor(&NSColor::clearColor());
        table.setStyle(NSTableViewStyle::Plain);
        table.setAllowsMultipleSelection(false);
        table.setAllowsEmptySelection(true);
        table.setSelectionHighlightStyle(NSTableViewSelectionHighlightStyle::None);
        table.setFocusRingType(NSFocusRingType::None);
        table.setGridStyleMask(NSTableViewGridLineStyle::empty());
        table.setColumnAutoresizingStyle(NSTableViewColumnAutoresizingStyle::NoColumnAutoresizing);
        let column =
            NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), ns_string!("content"));
        column.setWidth(bounds.size.width - if kind == 4 { 12.0 } else { 0.0 });
        column.setMinWidth(20.0);
        table.addTableColumn(&column);
        let vars = SourceVars::default();
        vars.kind.set(kind);
        let this = Source::alloc(mtm).set_ivars(vars);
        let source: Retained<Source> = unsafe { msg_send![super(this), init] };
        unsafe {
            table.setDataSource(Some(ProtocolObject::from_ref(&*source)));
            table.setDelegate(Some(ProtocolObject::from_ref(&*source)));
            table.setTarget(Some(&source));
            table.setAction(Some(objc2::sel!(clicked:)));
        }
        let scroll = widgets::scroll(bounds, mtm);
        scroll.setDocumentView(Some(&table));
        Self {
            scroll,
            table,
            source,
        }
    }

    pub fn replace(&self, rows: Vec<Row>) {
        *self.source.ivars().rows.borrow_mut() = rows;
        self.table.reloadData();
    }

    pub fn key(&self, index: isize) -> Option<String> {
        self.source
            .ivars()
            .rows
            .borrow()
            .get(index as usize)
            .map(|row| row.key.clone())
    }
}
