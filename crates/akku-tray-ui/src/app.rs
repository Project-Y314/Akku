//! The Akku tray window: device cards, controls, settings and the tray loop.

#![allow(unused_imports, unused_variables, dead_code, unreachable_patterns)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use eframe::egui;
use eframe::egui::{
    Align, Button, CentralPanel, Color32, ComboBox, Context, CornerRadius, Frame, Id, Layout,
    Margin, Rect, RichText, ScrollArea, Sense, Slider, Stroke, Ui, Vec2, ViewportCommand,
};

use akku_core::{
    load_bundled, Axis, BatteryState, Capabilities, CapabilityName, DeviceDescriptor, DeviceHealth,
    DeviceId, DeviceKind, DeviceState, DpiState, FieldStatus, LightingEffect, LightingState,
    PollingSpec, Settings, SettingsStore, Theme,
};
use akku_display::MonitorInfo;
use akku_hid::DeviceSnapshot;

use crate::autostart;
use crate::demo::{DemoWorker, Runtime};
use crate::{icon, tray};

/// The grayscale palette derived strictly from the two themes.
#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    card: Color32,
    rail: Color32,
    border: Color32,
    fg: Color32,
    dim: Color32,
    emerald: Color32,
    red: Color32,
    amber: Color32,
}

impl Palette {
    fn black() -> Self {
        Self {
            bg: Color32::BLACK,
            card: Color32::from_gray(16),
            rail: Color32::from_gray(34),
            border: Color32::from_gray(52),
            fg: Color32::WHITE,
            dim: Color32::from_gray(150),
            emerald: Color32::from_rgb(52, 211, 153),
            red: Color32::from_rgb(248, 113, 113),
            amber: Color32::from_rgb(251, 191, 36),
        }
    }

    fn white() -> Self {
        Self {
            bg: Color32::WHITE,
            card: Color32::from_gray(244),
            rail: Color32::from_gray(226),
            border: Color32::from_gray(199),
            fg: Color32::BLACK,
            dim: Color32::from_gray(96),
            emerald: Color32::from_rgb(52, 211, 153),
            red: Color32::from_rgb(248, 113, 113),
            amber: Color32::from_rgb(251, 191, 36),
        }
    }

    fn of(theme: Theme) -> Self {
        match theme {
            Theme::Black => Self::black(),
            Theme::White => Self::white(),
        }
    }

    fn invert_text(&self) -> Color32 {
        if self.fg == Color32::WHITE {
            Color32::BLACK
        } else {
            Color32::WHITE
        }
    }
}

fn pure_visuals(theme: Theme) -> egui::Visuals {
    let palette = Palette::of(theme);
    let mut visuals = match theme {
        Theme::Black => egui::Visuals::dark(),
        Theme::White => egui::Visuals::light(),
    };
    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.bg;
    visuals.extreme_bg_color = palette.rail;
    visuals.faint_bg_color = palette.card;
    visuals.override_text_color = Some(palette.fg);

    let base = egui::style::WidgetVisuals {
        bg_fill: palette.card,
        weak_bg_fill: palette.card,
        bg_stroke: Stroke::new(0.75, palette.border),
        fg_stroke: Stroke::new(1.0, palette.fg),
        expansion: 0.0,
        corner_radius: CornerRadius::same(4),
    };

    visuals.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: palette.bg,
        weak_bg_fill: palette.bg,
        bg_stroke: Stroke::new(0.75, palette.border),
        fg_stroke: Stroke::new(1.0, palette.fg),
        ..base
    };
    visuals.widgets.inactive = base;
    visuals.widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: palette.rail,
        weak_bg_fill: palette.rail,
        bg_stroke: Stroke::new(1.0, palette.border),
        ..base
    };
    visuals.widgets.active = egui::style::WidgetVisuals {
        bg_fill: palette.fg,
        weak_bg_fill: palette.fg,
        bg_stroke: Stroke::new(1.0, palette.fg),
        fg_stroke: Stroke::new(1.0, palette.invert_text()),
        ..base
    };
    visuals.selection = egui::style::Selection {
        bg_fill: palette.border,
        stroke: Stroke::new(1.0, palette.fg),
    };
    visuals
}

pub struct AkkuApp {
    store: SettingsStore,
    settings: Settings,
    worker: Runtime,
    demo: bool,
    start_hidden: bool,
    ctx: Option<Context>,
    window_visible: bool,
    applied_theme: Option<Theme>,
    snapshots: Vec<DeviceSnapshot>,
    status: Option<String>,
    tray: Option<tray::TrayHandles>,
    tray_icon_state: Option<(u8, bool)>,
    slider_edits: HashMap<String, f32>,
    dpi_manual_edits: HashMap<String, String>,
    monitors: Option<Vec<MonitorInfo>>,
    monitor_target: BTreeMap<String, u32>,
    monitor_result: BTreeMap<String, (bool, String)>,
    persist_refresh_checked: bool,
    scan_needed: bool,
    want_quit: bool,
}

impl AkkuApp {
    pub fn new(
        store: SettingsStore,
        settings: Settings,
        worker: Runtime,
        demo: bool,
        start_hidden: bool,
    ) -> Self {
        Self {
            store,
            settings,
            worker,
            demo,
            start_hidden,
            ctx: None,
            window_visible: true,
            applied_theme: None,
            snapshots: Vec::new(),
            status: None,
            tray: None,
            tray_icon_state: None,
            slider_edits: HashMap::new(),
            dpi_manual_edits: HashMap::new(),
            monitors: None,
            monitor_target: BTreeMap::new(),
            monitor_result: BTreeMap::new(),
            persist_refresh_checked: false,
            scan_needed: !demo,
            want_quit: false,
        }
    }

    pub fn preload_ctx(&mut self, ctx: &Context) {
        self.ctx = Some(ctx.clone());
        ctx.set_visuals(pure_visuals(self.settings.theme));
        self.applied_theme = Some(self.settings.theme);
    }

    pub fn install_tray(&mut self) {
        match tray::build() {
            Ok(handles) => self.tray = Some(handles),
            Err(error) => self.status = Some(format!("TRAY UNAVAILABLE: {error}")),
        }
    }

    // ---- event intake -----------------------------------------------------

    fn process_worker_events(&mut self) {
        while let Some(event) = self.worker.try_recv() {
            match event {
                akku_hid::WorkerEvent::Devices(devices) => {
                    self.snapshots = devices;
                    self.update_tray();
                }
                akku_hid::WorkerEvent::Message(text) => {
                    if !text.is_empty() {
                        self.status = Some(text);
                    }
                }
            }
        }
    }

    fn process_tray_events(&mut self, ctx: &Context) {
        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            let Some(handle) = &self.tray else {
                continue;
            };
            if event.id == handle.refresh_id {
                self.refresh_now();
            } else if event.id == handle.settings_id {
                self.open_settings(ctx);
            } else if event.id == handle.open_log_id {
                self.open_log();
            } else if event.id == handle.autostart_id {
                self.toggle_autostart();
            } else if event.id == handle.quit_id {
                self.want_quit = true;
            }
        }
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                ..
            } = event
            {
                self.toggle_window(ctx);
            }
        }
    }

    fn toggle_window(&mut self, ctx: &Context) {
        self.window_visible = !self.window_visible;
        ctx.send_viewport_cmd(ViewportCommand::Visible(self.window_visible));
        if self.window_visible {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
        ctx.request_repaint();
    }

    fn open_settings(&mut self, ctx: &Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.window_visible = true;
        // The settings window is opened via Tauri IPC in the tray handler
    }

    fn open_log(&mut self) {
        // TODO: open log file
        self.status = Some("LOG OPEN REQUESTED".into());
    }

    fn toggle_autostart(&mut self) {
        let new_value = !self.settings.autostart;
        match autostart::set_enabled(new_value) {
            Ok(()) => {
                self.settings.autostart = new_value;
                self.save_settings();
                self.status = Some(if new_value {
                    "AUTOSTART ENABLED".into()
                } else {
                    "AUTOSTART DISABLED".into()
                });
            }
            Err(error) => {
                self.status = Some(format!("AUTO-START FAILED: {error}"));
            }
        }
    }

    fn refresh_now(&mut self) {
        self.worker.send(akku_hid::WorkerCommand::Refresh);
        self.status = Some("REFRESH REQUESTED".into());
    }

    fn update_tray(&mut self) {
        let Some(handle) = &self.tray else {
            return;
        };
        let _ = handle.tray.set_tooltip(Some(tooltip_text(&self.snapshots)));

        // Use the first device with a battery percent for the tray icon
        let Some(primary) = self
            .snapshots
            .iter()
            .find(|d| d.battery.report.percent.is_some())
        else {
            return;
        };
        let percent = primary.battery.report.percent.unwrap_or(0);
        let charging = primary.battery.report.charging;
        if self.tray_icon_state != Some((percent, charging)) {
            self.tray_icon_state = Some((percent, charging));
            let rgba = icon::battery_rgba(Some(percent), charging);
            if let Ok(new_icon) = tray_icon::Icon::from_rgba(rgba, icon::SIZE, icon::SIZE) {
                let _ = handle.tray.set_icon(Some(new_icon));
            }
        }
    }

    fn save_settings(&self) {
        if let Err(error) = self.store.save(&self.settings) {
            eprintln!("akku: settings save failed: {error}");
        }
    }

    fn respawn_worker(&mut self, poll_secs: u64) {
        self.worker.shutdown();
        self.worker = if self.demo {
            Runtime::Demo(DemoWorker::spawn())
        } else {
            Runtime::Real(akku_hid::HidWorker::spawn(
                load_bundled().unwrap_or_default(),
                Duration::from_secs(poll_secs.max(5)),
                Arc::new(|| {}),
            ))
        };
    }

    fn apply_poll_interval(&mut self, secs: u64) {
        self.settings.poll_interval_secs = secs;
        self.save_settings();
        self.respawn_worker(secs);
        self.status = Some(format!("POLL INTERVAL -> {secs} S"));
    }

    fn toggle_theme(&mut self, ctx: &Context) {
        let next = self.settings.theme.toggled();
        let prev = std::mem::replace(&mut self.settings.theme, next);
        if prev == next {
            return;
        }
        self.save_settings();
        let visuals = pure_visuals(next);
        ctx.set_visuals(visuals);
        self.applied_theme = Some(next);
    }

    fn scan_monitors(&mut self) {
        self.scan_needed = false;
        match akku_display::list_monitors() {
            Ok(monitors) => self.monitors = Some(monitors),
            Err(error) => {
                self.monitors = None;
                self.status = Some(format!("DISPLAY SCAN FAILED: {error}"));
            }
        }
    }

    fn apply_refresh(&mut self, monitor: &MonitorInfo) {
        let target = self
            .monitor_target
            .get(&monitor.id)
            .copied()
            .unwrap_or(monitor.current.refresh_hz);
        let verdict =
            match akku_display::set_refresh(&monitor.id, target, self.persist_refresh_checked) {
                Ok(mode) => (true, format!("{} applied", mode)),
                Err(error) => (false, format!("set failed: {error}")),
            };
        self.monitor_result.insert(monitor.id.clone(), verdict);
    }

    // ---- rendering --------------------------------------------------------

    fn render(&mut self, ui: &mut Ui) {
        let palette = Palette::of(self.settings.theme);

        ui.horizontal(|ui| {
            ui.label(RichText::new("AKKU").size(22.0).strong());
            ui.add_space(4.0);
            let tag = if self.demo { "DEMO" } else { "DEVICE HUB" };
            ui.label(RichText::new(tag).monospace().size(9.0).color(palette.dim));
            ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                let theme_label = self.settings.theme.label();
                let button = Button::new(
                    RichText::new(theme_label)
                        .monospace()
                        .size(10.0)
                        .color(palette.fg),
                )
                .fill(palette.card)
                .stroke(Stroke::new(1.0, palette.border))
                .corner_radius(CornerRadius::same(4));
                if ui.add(button).clicked() {
                    self.toggle_theme(ui.ctx());
                }
            });
        });
        ui.add_space(12.0);

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.snapshots.is_empty() {
                    self.draw_empty(ui);
                } else {
                    let snapshots = self.snapshots.clone();
                    for snapshot in snapshots {
                        self.draw_device_card(ui, &snapshot);
                    }
                }
                ui.add_space(12.0);
                self.draw_settings_card(ui);
            });

        self.draw_footer(ui);
    }

    fn draw_empty(&mut self, ui: &mut Ui) {
        let palette = Palette::of(self.settings.theme);
        let frame = Frame::new()
            .inner_margin(Margin::same(20))
            .fill(palette.card)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(CornerRadius::same(6));
        let hint = if self.demo {
            "DEMO: the virtual devices should appear within a second".to_string()
        } else {
            "NO DEVICES MATCHED A MANIFEST.\nConnect the Attack Shark receiver or the Epomaker\nP65, or wait for the next scan.".to_string()
        };
        frame.show(ui, |ui| {
            ui.label(
                RichText::new(hint)
                    .monospace()
                    .size(11.0)
                    .color(palette.dim),
            );
        });
    }

    fn card_frame(&self) -> Frame {
        let palette = Palette::of(self.settings.theme);
        Frame::new()
            .inner_margin(Margin::same(16))
            .outer_margin(Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: 12,
            })
            .fill(palette.card)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(CornerRadius::same(6))
    }

    fn section(&self, ui: &mut Ui, label: &str) {
        let palette = Palette::of(self.settings.theme);
        ui.add_space(10.0);
        ui.label(
            RichText::new(label)
                .monospace()
                .size(9.0)
                .color(palette.dim),
        );
        ui.add_space(2.0);
    }

    fn draw_device_card(&mut self, ui: &mut Ui, snapshot: &DeviceSnapshot) {
        let frame = self.card_frame();
        frame.show(ui, |inner| {
            self.device_body(inner, snapshot);
        });
    }

    fn device_body(&mut self, ui: &mut Ui, snapshot: &DeviceSnapshot) {
        let palette = Palette::of(self.settings.theme);
        let id = snapshot.manifest_id.0.clone();

        ui.horizontal(|ui| {
            let tags = format!(
                "{} · {} · {}",
                snapshot.kind.label(),
                snapshot.connection.label(),
                if snapshot.writable {
                    "WRITES ON"
                } else {
                    "READ-ONLY"
                }
            );
            ui.label(RichText::new(tags).monospace().size(9.0).color(palette.dim));
            ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                ui.label(
                    RichText::new(format!("{:?}", snapshot.state.health))
                        .monospace()
                        .size(9.0)
                        .color(palette.dim),
                );
            });
        });
        ui.add_space(4.0);
        ui.label(RichText::new(snapshot.name.clone()).size(17.0).strong());
        ui.label(
            RichText::new(format!("{} · {}", snapshot.vendor, snapshot.key.label()))
                .monospace()
                .size(9.0)
                .color(palette.dim),
        );

        self.section(ui, "BATTERY");
        self.battery_bar(ui, snapshot);

        if snapshot.capabilities.dpi && snapshot.dpi_values.is_some() {
            self.section(ui, "DPI");
            self.dpi_controls(ui, snapshot);
        }

        if snapshot.capabilities.polling_rate && snapshot.polling_rates.is_some() {
            self.section(ui, "POLLING RATE");
            self.polling_controls(ui, snapshot);
        }

        if snapshot.capabilities.actuation && snapshot.actuation_range.is_some() {
            self.section(ui, "ACTUATION POINT");
            self.actuation_controls(ui, snapshot);
        }

        if let Some(error) = snapshot.last_error.clone() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("LAST ERROR: {error}"))
                    .monospace()
                    .size(9.0)
                    .color(palette.dim),
            );
        }
        if !snapshot.writable {
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "READ-ONLY · writes unlock after a verified hardware capture (writes_verified)",
                )
                .monospace()
                .size(9.0)
                .color(palette.dim),
            );
        }
    }

    fn battery_bar(&self, ui: &mut Ui, snapshot: &DeviceSnapshot) {
        let palette = Palette::of(self.settings.theme);
        let ratio = snapshot
            .battery
            .report
            .percent
            .map(|percent| f32::from(percent) / 100.0)
            .unwrap_or(0.0);
        let label = snapshot
            .battery
            .report
            .percent
            .map(|percent| format!("{percent}%"))
            .unwrap_or_else(|| "--".into());

        ui.horizontal(|ui| {
            let width = (ui.available_width() - 86.0).max(60.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::same(3), palette.rail);
            let portion = ratio.clamp(0.02, 1.0);
            if ratio > 0.0 {
                let fill = Rect::from_min_size(rect.min, Vec2::new(width * portion, rect.height()));
                let fill_color = if snapshot.battery.report.charging {
                    palette.emerald
                } else if ratio <= 0.15 {
                    palette.red
                } else if ratio <= 0.30 {
                    palette.amber
                } else {
                    palette.fg
                };
                painter.rect_filled(fill, CornerRadius::same(3), fill_color);
            }
            painter.rect_stroke(
                rect,
                CornerRadius::same(3),
                Stroke::new(1.0, palette.border),
                egui::StrokeKind::Inside,
            );

            ui.label(
                RichText::new(label)
                    .monospace()
                    .size(12.0)
                    .color(palette.fg),
            );
            if snapshot.battery.report.charging {
                ui.label(
                    RichText::new("CHARGING")
                        .monospace()
                        .size(9.0)
                        .color(palette.emerald),
                );
            }
            if let Some(ago) = updated_ago(snapshot.battery.updated_at) {
                ui.label(
                    RichText::new(ago.to_string())
                        .monospace()
                        .size(9.0)
                        .color(palette.dim),
                );
            }
        });
        if snapshot.battery.stale {
            ui.label(
                RichText::new("STALE · no fresh report from the device")
                    .monospace()
                    .size(9.0)
                    .color(palette.dim),
            );
        }
    }

    fn dpi_controls(&mut self, ui: &mut Ui, snapshot: &DeviceSnapshot) {
        let palette = Palette::of(self.settings.theme);
        let values = snapshot.dpi_values.clone().unwrap_or_default();
        let values_for_closure = values.clone();
        let current = snapshot.dpi;

        ui.horizontal_wrapped(|ui| {
            for value in values_for_closure {
                let selected = current == Some(value);
                let response = self.chip(ui, &value.to_string(), selected, &palette);
                if response.clicked() && !selected {
                    self.worker.send(akku_hid::WorkerCommand::SetDpi {
                        manifest_id: snapshot.manifest_id.0.clone(),
                        dpi: value,
                    });
                    self.status = Some("WRITE REQUESTED".into());
                }
            }
        });
        ui.add_space(2.0);

        // Manual DPI entry
        let entry = self
            .dpi_manual_edits
            .entry(format!("{}_dpi_manual", snapshot.manifest_id.0))
            .or_insert(
                snapshot
                    .dpi
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| values.first().copied().unwrap_or(800).to_string()),
            );
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Custom:")
                    .monospace()
                    .size(10.0)
                    .color(palette.dim),
            );
            let response = ui.add(
                egui::TextEdit::singleline(entry)
                    .desired_width(80.0)
                    .font(egui::FontId::monospace(11.0)),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Ok(dpi) = entry.trim().parse::<u16>() {
                    self.worker.send(akku_hid::WorkerCommand::SetDpi {
                        manifest_id: snapshot.manifest_id.0.clone(),
                        dpi,
                    });
                    self.status = Some(format!("SET DPI {dpi} REQUESTED"));
                }
            }
        });
    }

    fn polling_controls(&mut self, ui: &mut Ui, snapshot: &DeviceSnapshot) {
        let palette = Palette::of(self.settings.theme);
        let rates = snapshot.polling_rates.clone().unwrap_or_default();
        let current = snapshot.polling_hz;

        ui.horizontal_wrapped(|ui| {
            for value in rates {
                let selected = current == Some(value);
                let response = self.chip(ui, &format!("{} Hz", value), selected, &palette);
                if response.clicked() && !selected {
                    self.worker.send(akku_hid::WorkerCommand::SetPolling {
                        manifest_id: snapshot.manifest_id.0.clone(),
                        hz: value,
                    });
                    self.status = Some("WRITE REQUESTED".into());
                }
            }
        });
    }

    fn actuation_controls(&mut self, ui: &mut Ui, snapshot: &DeviceSnapshot) {
        let palette = Palette::of(self.settings.theme);
        let spec = snapshot.actuation_range.clone().unwrap();
        let entry = self
            .slider_edits
            .entry(snapshot.manifest_id.0.clone())
            .or_insert(snapshot.actuation_mm.unwrap_or(spec.min_mm));
        let display = format!("{:.1} MM", *entry);
        let response = ui.add(
            Slider::new(entry, spec.min_mm..=spec.max_mm)
                .step_by(f64::from(spec.step_mm))
                .show_value(false)
                .text(display),
        );
        let commit = response.drag_stopped() || (response.changed() && !response.dragged());
        if commit {
            let millimetres = *entry;
            self.worker.send(akku_hid::WorkerCommand::SetActuation {
                manifest_id: snapshot.manifest_id.0.clone(),
                millimetres,
            });
            self.status = Some(format!("SET ACTUATION {:.1} MM REQUESTED", millimetres));
        }
    }

    fn chip(&self, ui: &mut Ui, label: &str, selected: bool, palette: &Palette) -> egui::Response {
        let (fill, text_color, stroke) = if selected {
            (palette.fg, palette.bg, Stroke::new(1.0, palette.fg))
        } else {
            (palette.card, palette.fg, Stroke::new(1.0, palette.border))
        };
        let button = Button::new(
            RichText::new(label)
                .monospace()
                .size(11.0)
                .color(text_color),
        )
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(4))
        .min_size(Vec2::new(0.0, 22.0));
        ui.add(button)
    }

    fn draw_settings_card(&mut self, ui: &mut Ui) {
        let frame = self.card_frame();
        frame.show(ui, |inner| self.settings_body(inner));
    }

    fn settings_body(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("SETTINGS").size(13.0).strong());

        self.section(ui, "POLL INTERVAL");
        let mut secs = self.settings.poll_interval_secs.clamp(30, 300) as f32;
        let display = format!("{} S", secs.round() as u64);
        let response = ui.add(
            Slider::new(&mut secs, 30.0..=300.0)
                .step_by(5.0)
                .show_value(false)
                .text(display),
        );
        if response.drag_stopped() || (response.changed() && !response.dragged()) {
            let rounded_secs = secs.round() as u64;
            if rounded_secs != self.settings.poll_interval_secs {
                self.apply_poll_interval(rounded_secs);
            }
        }

        self.section(ui, "AUTO-START");
        let mut autostart_value = self.settings.autostart;
        if ui
            .checkbox(
                &mut autostart_value,
                RichText::new("start akku hidden when windows starts")
                    .monospace()
                    .size(10.0),
            )
            .changed()
        {
            match autostart::set_enabled(autostart_value) {
                Ok(()) => {
                    self.settings.autostart = autostart_value;
                    self.save_settings();
                    self.status = Some(if autostart_value {
                        "AUTOSTART ENABLED".into()
                    } else {
                        "AUTOSTART DISABLED".into()
                    });
                }
                Err(error) => {
                    self.status = Some(format!("AUTO-START FAILED: {error}"));
                }
            }
        }

        self.section(ui, "DISPLAY REFRESH");
        if self.scan_needed {
            self.scan_monitors();
        }
        match self.monitors.clone() {
            Some(monitors) => {
                for monitor in monitors {
                    let rates = rates_at_current_resolution(&monitor);
                    if rates.is_empty() {
                        continue;
                    }
                    let mut target = self
                        .monitor_target
                        .get(&monitor.id)
                        .copied()
                        .unwrap_or(monitor.current.refresh_hz);
                    let mut apply_requested = false;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(monitor.label.clone()).monospace().size(10.0));
                        ComboBox::new(Id::unique(format!("refresh-{}", monitor.id)), "").show_ui(
                            ui,
                            |ui| {
                                for rate in &rates {
                                    ui.selectable_value(
                                        &mut target,
                                        *rate,
                                        RichText::new(format!("{rate} Hz")).monospace(),
                                    );
                                }
                            },
                        );
                        let palette = Palette::of(self.settings.theme);
                        let button = Button::new(
                            RichText::new("APPLY")
                                .monospace()
                                .size(10.0)
                                .color(palette.fg),
                        )
                        .fill(palette.card)
                        .stroke(Stroke::new(1.0, palette.border))
                        .corner_radius(CornerRadius::same(4));
                        if ui.add(button).clicked() {
                            apply_requested = true;
                        }
                    });
                    self.monitor_target.insert(monitor.id.clone(), target);
                    if apply_requested {
                        self.apply_refresh(&monitor);
                    }
                    if let Some((ok, text)) = self.monitor_result.get(&monitor.id).cloned() {
                        ui.label(
                            RichText::new((if ok { "OK · " } else { "" }).to_string() + &text)
                                .monospace()
                                .size(9.0)
                                .color(Palette::of(self.settings.theme).dim),
                        );
                    }
                }
                let mut persist = self.persist_refresh_checked;
                if ui
                    .checkbox(
                        &mut persist,
                        RichText::new("persist after reboot").monospace().size(10.0),
                    )
                    .changed()
                {
                    self.persist_refresh_checked = persist;
                }
            }
            None => {
                ui.label(RichText::new("no displays scanned").monospace().size(9.0));
            }
        }

        ui.add_space(14.0);
        self.section(ui, "ACTIONS");
        ui.horizontal(|ui| {
            let palette = Palette::of(self.settings.theme);
            let refresh_button = Button::new(
                RichText::new("REFRESH NOW")
                    .monospace()
                    .size(10.0)
                    .color(palette.fg),
            )
            .fill(palette.rail)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(CornerRadius::same(4));
            if ui.add(refresh_button).clicked() {
                self.refresh_now();
            }
            let quit_button = Button::new(
                RichText::new("QUIT")
                    .monospace()
                    .size(10.0)
                    .color(palette.fg),
            )
            .fill(palette.card)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(CornerRadius::same(4));
            if ui.add(quit_button).clicked() {
                self.want_quit = true;
            }
        });
    }

    fn draw_footer(&mut self, ui: &mut Ui) {
        let palette = Palette::of(self.settings.theme);
        ui.add_space(6.0);
        let line = match &self.status {
            Some(text) => text.as_str(),
            None => {
                if self.snapshots.is_empty() {
                    "WAITING FOR DEVICE REPORTS"
                } else if self.snapshots.iter().all(|d| !d.battery.stale) {
                    "ALL DEVICES REPORTING"
                } else {
                    "SOME DEVICE REPORTS ARE STALE"
                }
            }
        };
        ui.label(RichText::new(line).monospace().size(9.0).color(palette.dim));
    }
}

impl eframe::App for AkkuApp {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if self.applied_theme != Some(self.settings.theme) {
            let visuals = pure_visuals(self.settings.theme);
            ctx.set_visuals(visuals);
            self.applied_theme = Some(self.settings.theme);
        }
        if self.start_hidden {
            self.start_hidden = false;
            self.window_visible = false;
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        self.process_worker_events();
        self.process_tray_events(ctx);
        if self.want_quit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        } else if ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.window_visible = false;
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        ctx.request_repaint_after(Duration::from_millis(400));
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        CentralPanel::default()
            .frame(
                Frame::new()
                    .inner_margin(Margin::same(18))
                    .fill(Palette::of(self.settings.theme).bg),
            )
            .show(ui, |panel| self.render(panel));
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        match self.settings.theme {
            Theme::Black => [0.0, 0.0, 0.0, 1.0],
            Theme::White => [1.0, 1.0, 1.0, 1.0],
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.worker.shutdown();
        let _ = self.store.save(&self.settings);
        self.tray = None;
    }
}

// ---- helpers --------------------------------------------------------------

fn tooltip_text(snapshots: &[DeviceSnapshot]) -> String {
    if snapshots.is_empty() {
        return "PeripheralDeck — no devices connected".into();
    }
    let parts: Vec<String> = snapshots
        .iter()
        .map(|snapshot| {
            let short_name = snapshot
                .name
                .split('(')
                .next()
                .unwrap_or(&snapshot.name)
                .trim();
            match snapshot.battery.report.percent {
                Some(percent) => format!("{short_name} {percent}%"),
                None => format!("{short_name} --"),
            }
        })
        .collect();
    format!("PeripheralDeck · {}", parts.join("  ·  "))
}

fn updated_ago(updated_at: SystemTime) -> Option<String> {
    let elapsed = SystemTime::now().duration_since(updated_at).ok()?;
    match elapsed.as_secs() {
        0 => None,
        1..=59 => Some(format!("{} s ago", elapsed.as_secs())),
        secs => Some(format!("{} min ago", secs / 60)),
    }
}

fn rates_at_current_resolution(monitor: &MonitorInfo) -> Vec<u32> {
    let mut rates: Vec<u32> = monitor
        .modes
        .iter()
        .filter(|mode| mode.width == monitor.current.width && mode.height == monitor.current.height)
        .map(|mode| mode.refresh_hz)
        .collect();
    rates.sort_unstable();
    rates.dedup();
    rates
}
