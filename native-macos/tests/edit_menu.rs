// Exercise the actual AppKit menu builder on the process main thread. This
// checks menu wiring/lifetimes, not keyboard delivery to a nonactivating panel.
#[cfg(target_os = "macos")]
#[path = "../src/app/menu.rs"]
mod menu;

#[cfg(target_os = "macos")]
fn main() {
    use objc2::sel;
    use objc2_app_kit::{NSApplication, NSEventModifierFlags};
    use objc2_foundation::MainThreadMarker;

    let mtm = MainThreadMarker::new().expect("AppKit regression checks need the main thread");
    let app = NSApplication::sharedApplication(mtm);
    menu::install(&app);

    // The builder has returned, dropping all its local Retained values.
    let installed = app.mainMenu().expect("a main menu must be installed");
    assert!(installed.numberOfItems() >= 2);
    let application_item = installed.itemAtIndex(0).expect("application menu slot");
    assert!(application_item.submenu().is_some());
    let edit_item = installed
        .itemArray()
        .into_iter()
        .find(|item| item.title().to_string() == "Edit")
        .expect("Edit menu slot");
    let edit = edit_item.submenu().expect("Edit submenu must stay alive");
    assert!(edit.autoenablesItems(), "AppKit must validate commands");

    let command = NSEventModifierFlags::Command;
    let expected = [
        (sel!(undo:), "z", command),
        (sel!(redo:), "z", command | NSEventModifierFlags::Shift),
        (sel!(cut:), "x", command),
        (sel!(copy:), "c", command),
        (sel!(paste:), "v", command),
        (sel!(selectAll:), "a", command),
    ];
    let commands: Vec<_> = edit
        .itemArray()
        .into_iter()
        .filter(|item| !item.isSeparatorItem())
        .collect();
    for (action, key, modifiers) in expected {
        let matching: Vec<_> = commands
            .iter()
            .filter(|item| item.action() == Some(action))
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "each edit action must be registered once"
        );
        let item = matching[0];
        assert_eq!(item.keyEquivalent().to_string(), key);
        assert_eq!(item.keyEquivalentModifierMask(), modifiers);
        assert!(
            item.target().is_none(),
            "commands must use the responder chain"
        );
    }

    // Do not register Edit equivalents for the existing Enter/Esc path.
    assert!(!commands
        .iter()
        .any(|item| { matches!(item.keyEquivalent().to_string().as_str(), "\r" | "\u{1b}") }));
    println!("AppKit Edit menu checks passed (installation, lifetime, selectors, modifiers, nil targets)");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("SKIPPED: AppKit Edit menu checks require macOS; no keyboard behavior was tested");
}
