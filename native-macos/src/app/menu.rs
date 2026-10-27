use objc2::{sel, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
use objc2_foundation::ns_string;

pub(super) fn install(app: &NSApplication) {
    let mtm = app.mtm();
    let main = NSMenu::new(mtm);

    // AppKit reserves the first top-level slot for the application menu.
    // Keep quitting available without introducing another keyboard shortcut.
    let application = NSMenu::new(mtm);
    let quit = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Quit SimpleT Native"),
            Some(sel!(terminate:)),
            ns_string!(""),
        )
    };
    application.addItem(&quit);
    let application_item = NSMenuItem::new(mtm);
    application_item.setSubmenu(Some(&application));
    main.addItem(&application_item);

    let edit = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Edit"));
    // Let AppKit validate selection, editability and the current undo manager.
    edit.setAutoenablesItems(true);
    let command = NSEventModifierFlags::Command;
    for (index, (title, action, key, modifiers)) in [
        (ns_string!("Undo"), sel!(undo:), ns_string!("z"), command),
        (
            ns_string!("Redo"),
            sel!(redo:),
            ns_string!("z"),
            command | NSEventModifierFlags::Shift,
        ),
        (ns_string!("Cut"), sel!(cut:), ns_string!("x"), command),
        (ns_string!("Copy"), sel!(copy:), ns_string!("c"), command),
        (ns_string!("Paste"), sel!(paste:), ns_string!("v"), command),
        (
            ns_string!("Select All"),
            sel!(selectAll:),
            ns_string!("a"),
            command,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if index == 2 {
            edit.addItem(&NSMenuItem::separatorItem(mtm));
        }
        // These standard AppKit actions take the menu item as sender. A nil
        // target lets NSApplication find the field editor / first responder,
        // rather than invoking the app's whole-text clipboard buttons.
        let item = unsafe {
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                title,
                Some(action),
                key,
            );
            item.setTarget(None);
            item
        };
        item.setKeyEquivalentModifierMask(modifiers);
        edit.addItem(&item);
    }
    let edit_item = NSMenuItem::new(mtm);
    edit_item.setTitle(ns_string!("Edit"));
    edit_item.setSubmenu(Some(&edit));
    main.addItem(&edit_item);

    // NSApplication retains the main menu; menus retain their items/submenus.
    app.setMainMenu(Some(&main));
}
