//! `akku` — tray application entry point.
//!
//! Flags:
//! - `--demo`    run against deterministic virtual devices (no hardware)
//! - `--hidden`  start minimized to the tray (used by the autostart entry)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod demo;
mod icon;
mod tray;

use std::sync::Arc;
use std::time::Duration;

use app::AkkuApp;
use demo::{DemoWorker, Runtime};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let demo_mode = args.iter().any(|arg| arg == "--demo");
    let start_hidden = args.iter().any(|arg| arg == "--hidden");

    let store = akku_core::SettingsStore::new(akku_core::default_settings_path());
    let settings = store.load();

    let worker = if demo_mode {
        eprintln!("akku: --demo — driving deterministic virtual devices");
        Runtime::Demo(DemoWorker::spawn())
    } else {
        let manifests = match akku_core::load_bundled() {
            Ok(manifests) => manifests,
            Err(error) => {
                eprintln!("akku: bundled manifests are invalid: {error}");
                return;
            }
        };
        Runtime::Real(akku_hid::HidWorker::spawn(
            manifests,
            Duration::from_secs(settings.poll_interval_secs.max(5)),
            Arc::new(|| {}),
        ))
    };

    let window_icon = window_icon();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_app_id("akku")
            .with_title("akku")
            .with_icon(window_icon)
            .with_inner_size(eframe::egui::vec2(400.0, 520.0))
            .with_min_inner_size(eframe::egui::vec2(360.0, 460.0)),
        ..Default::default()
    };

    let result = eframe::run_native(
        "akku",
        options,
        Box::new(move |cc| {
            let mut app = AkkuApp::new(store, settings, worker, demo_mode, start_hidden);
            app.preload_ctx(&cc.egui_ctx);
            app.install_tray();
            Ok(Box::new(app))
        }),
    );
    if let Err(error) = result {
        eprintln!("akku failed: {error}");
        std::process::exit(1);
    }
}

fn window_icon() -> eframe::egui::IconData {
    eframe::egui::IconData {
        width: icon::SIZE,
        height: icon::SIZE,
        rgba: icon::battery_rgba(None, false),
    }
}
