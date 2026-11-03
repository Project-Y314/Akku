//! Tray icon assembly plus the stable ids the event loop matches against.

use crate::icon;

/// Owned tray icon and the menu ids used to route menu events.
pub struct TrayHandles {
    pub tray: tray_icon::TrayIcon,
    pub show_id: tray_icon::menu::MenuId,
    pub refresh_id: tray_icon::menu::MenuId,
    pub quit_id: tray_icon::menu::MenuId,
}

pub fn build() -> Result<TrayHandles, String> {
    let menu = tray_icon::menu::Menu::new();
    let show = tray_icon::menu::MenuItem::new("Show / hide window", true, None);
    let refresh = tray_icon::menu::MenuItem::new("Refresh devices", true, None);
    let quit = tray_icon::menu::MenuItem::new("Quit akku", true, None);
    let separator = tray_icon::menu::PredefinedMenuItem::separator();

    menu.append(&show).map_err(|error| error.to_string())?;
    menu.append(&refresh).map_err(|error| error.to_string())?;
    menu.append(&separator).map_err(|error| error.to_string())?;
    menu.append(&quit).map_err(|error| error.to_string())?;

    let rgba = icon::battery_rgba(None, false);
    let tray_icon = tray_icon::Icon::from_rgba(rgba, icon::SIZE, icon::SIZE)
        .map_err(|error| format!("tray icon rejected: {error}"))?;

    let tray = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(tray_icon)
        .with_tooltip("akku — starting")
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| format!("tray icon unavailable: {error}"))?;

    Ok(TrayHandles {
        tray,
        show_id: show.id().clone(),
        refresh_id: refresh.id().clone(),
        quit_id: quit.id().clone(),
    })
}
