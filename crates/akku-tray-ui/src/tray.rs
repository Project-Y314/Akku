//! Tray icon assembly plus the stable ids the event loop matches against.

use crate::icon;

/// Owned tray icon and the menu ids used to route menu events.
pub struct TrayHandles {
    pub tray: tray_icon::TrayIcon,
    pub refresh_id: tray_icon::menu::MenuId,
    pub settings_id: tray_icon::menu::MenuId,
    pub open_log_id: tray_icon::menu::MenuId,
    pub autostart_id: tray_icon::menu::MenuId,
    pub quit_id: tray_icon::menu::MenuId,
}

pub fn build() -> Result<TrayHandles, String> {
    let menu = tray_icon::menu::Menu::new();
    let refresh = tray_icon::menu::MenuItem::new("Refresh", true, None);
    let settings = tray_icon::menu::MenuItem::new("Settings…", true, None);
    let open_log = tray_icon::menu::MenuItem::new("Open Log", true, None);
    let autostart = tray_icon::menu::CheckMenuItem::new("Start with system", true, false, None);
    let separator = tray_icon::menu::PredefinedMenuItem::separator();
    let quit = tray_icon::menu::MenuItem::new("Quit", true, None);

    menu.append(&refresh).map_err(|error| error.to_string())?;
    menu.append(&settings).map_err(|error| error.to_string())?;
    menu.append(&open_log).map_err(|error| error.to_string())?;
    menu.append(&autostart).map_err(|error| error.to_string())?;
    menu.append(&separator).map_err(|error| error.to_string())?;
    menu.append(&quit).map_err(|error| error.to_string())?;

    let rgba = icon::battery_rgba(None, false);
    let tray_icon = tray_icon::Icon::from_rgba(rgba, icon::SIZE, icon::SIZE)
        .map_err(|error| format!("tray icon rejected: {error}"))?;

    let tray = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(tray_icon)
        .with_tooltip("PeripheralDeck — starting")
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| format!("tray icon unavailable: {error}"))?;

    Ok(TrayHandles {
        tray,
        refresh_id: refresh.id().clone(),
        settings_id: settings.id().clone(),
        open_log_id: open_log.id().clone(),
        autostart_id: autostart.id().clone(),
        quit_id: quit.id().clone(),
    })
}
