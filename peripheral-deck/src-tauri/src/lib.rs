//! Tauri v2 application entry library and tray bootstrap for PeripheralDeck.
//!
//! Wires `peripheral-deck-core` types and the wide animated tray pill renderer into
//! the Tauri v2 system tray (`TrayIconBuilder`) with left-click animated popover toggle
//! and right-click context menu (`Refresh`, `Settings…`, `Open Log`, `Start with system`, `Quit`).

use std::sync::Mutex;

use peripheral_deck_core::{
    PopoverAnimationFrame, TrayAnimationController, TrayBadgeState, TrayPillDimensions,
    TrayPillRenderer,
};
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, State,
};

/// Shared tray state managed by Tauri.
pub struct AppTrayState {
    pub badge: Mutex<TrayBadgeState>,
    pub animation: Mutex<TrayAnimationController>,
}

impl Default for AppTrayState {
    fn default() -> Self {
        Self {
            badge: Mutex::new(TrayBadgeState {
                primary_device_name: None,
                battery_percent: None,
                charging: false,
                popover_open: false,
                animation_phase: 0.0,
            }),
            animation: Mutex::new(TrayAnimationController::new()),
        }
    }
}

#[tauri::command]
pub fn get_tray_badge(state: State<'_, AppTrayState>) -> Result<TrayBadgeState, String> {
    state
        .badge
        .lock()
        .map(|guard| guard.clone())
        .map_err(|_| "tray state lock poisoned".to_string())
}

#[tauri::command]
pub fn tick_popover_animation(
    dt_ms: f32,
    state: State<'_, AppTrayState>,
) -> Result<PopoverAnimationFrame, String> {
    let charging = state
        .badge
        .lock()
        .map_err(|_| "tray state lock poisoned".to_string())?
        .charging;
    let mut anim = state
        .animation
        .lock()
        .map_err(|_| "animation state lock poisoned".to_string())?;
    let frame = anim.tick(dt_ms, charging);
    if let Ok(mut badge) = state.badge.lock() {
        badge.animation_phase = anim.charging_phase();
    }
    Ok(frame)
}

/// Builds the Tauri v2 tray icon using the wide pill renderer (`64x22` on macOS/Linux
/// and `32x32` stacked percentage pill on Windows) and registers left/right click handlers.
pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let refresh_item = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let open_log_item = MenuItem::with_id(app, "open_log", "Open Log", true, None::<&str>)?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with system",
        true,
        false,
        None::<&str>,
    )?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &refresh_item,
            &settings_item,
            &open_log_item,
            &autostart_item,
            &sep,
            &quit_item,
        ],
    )?;

    let initial_badge = TrayBadgeState::default();
    let icon_image = render_platform_tray_image(&initial_badge);

    TrayIconBuilder::with_id("main-tray")
        .icon(icon_image)
        .tooltip(initial_badge.tooltip())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "refresh" => {
                let _ = app.emit("deck://refresh-requested", ());
            }
            "settings" => {
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "open_log" => {
                let _ = app.emit("deck://open-log-requested", ());
            }
            "autostart" => {
                let _ = app.emit("deck://toggle-autostart-requested", ());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(popover) = app.get_webview_window("popover") {
                    let is_visible = popover.is_visible().unwrap_or(false);
                    let next_open = !is_visible;
                    if let Some(state) = app.try_state::<AppTrayState>() {
                        if let Ok(mut anim) = state.animation.lock() {
                            anim.set_open(next_open);
                        }
                        if let Ok(mut badge) = state.badge.lock() {
                            badge.popover_open = next_open;
                            let _ = tray.set_icon(Some(render_platform_tray_image(&badge)));
                        }
                    }
                    if next_open {
                        let _ = popover.show();
                        let _ = popover.set_focus();
                        let _ = app.emit("deck://popover-animate", true);
                    } else {
                        let _ = app.emit("deck://popover-animate", false);
                        let _ = popover.hide();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn render_platform_tray_image(badge: &TrayBadgeState) -> Image<'static> {
    if cfg!(target_os = "windows") {
        let dims = TrayPillDimensions::COMPACT_SQUARE;
        let rgba = TrayPillRenderer::render_compact_square_rgba(badge);
        Image::new_owned(rgba, dims.width, dims.height)
    } else {
        let dims = TrayPillDimensions::WIDE_PILL;
        let rgba = TrayPillRenderer::render_wide_pill_rgba(badge);
        Image::new_owned(rgba, dims.width, dims.height)
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .manage(AppTrayState::default())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_tray_badge,
            tick_popover_animation
        ])
        .run(tauri::generate_context!())
        .expect("error while running PeripheralDeck Tauri application");
}
