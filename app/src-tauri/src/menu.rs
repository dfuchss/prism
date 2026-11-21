//! The macOS app menu: "About `PRism`" opens the About tab. Other platforms keep their default.

use tauri::menu::MenuEvent;
use tauri::{AppHandle, Runtime};

#[cfg(target_os = "macos")]
const ABOUT_ID: &str = "about";

/// Replaces the predefined About item of the app menu with one that shows the About tab.
#[cfg(target_os = "macos")]
pub(crate) fn install(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};
    let menu = Menu::default(app.handle())?;
    if let Some(MenuItemKind::Submenu(application)) = menu.items()?.into_iter().next() {
        if let Some(predefined) = application.items()?.into_iter().next() {
            application.remove(&predefined)?;
        }
        let about = MenuItem::with_id(app, ABOUT_ID, "About PRism", true, None::<&str>)?;
        application.insert(&about, 0)?;
    }
    app.set_menu(menu)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "same signature as the macOS variant"
)]
pub(crate) fn install(_app: &tauri::App) -> tauri::Result<()> {
    Ok(())
}

/// Shows the About tab when its menu item was chosen.
#[cfg(target_os = "macos")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands the event over by value"
)]
pub(crate) fn on_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    if event.id().as_ref() == ABOUT_ID {
        crate::events::show_about(app);
    }
}

/// No custom menu items on other platforms.
#[cfg(not(target_os = "macos"))]
pub(crate) fn on_event<R: Runtime>(_app: &AppHandle<R>, _event: MenuEvent) {}
