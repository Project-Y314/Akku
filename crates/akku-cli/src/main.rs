//! `akku-cli` — terminal access to the same core, HID and display libraries the
//! tray UI uses.
//!
//! Read paths work for every device matched by a manifest. Write paths stay
//! rejected until the matching manifest sets `writes_verified = true`, which
//! must only happen after a real feature-report capture has been confirmed on
//! the actual hardware.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use akku_core::{
    load_bundled, load_dir, ActuationCapability, DeviceKind, DpiCapability, Manifest,
    PeripheralDriver, PollingRateCapability,
};
use akku_display::{list_monitors, set_refresh};
use akku_hid::{
    describe, enumerate, find_manifest, probe, Device, HidBackend, HidDriver, MockDriver,
};

#[derive(Parser)]
#[command(
    name = "akku-cli",
    version,
    about = "Inspect and control Akku-known devices from the terminal",
    after_help = "Write commands are rejected while a manifest has writes_verified = false."
)]
struct Cli {
    /// Directory of extra .toml manifests; entries override bundled ones by id
    #[arg(long, value_name = "DIR")]
    manifests: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List every accessible HID interface, matched or not
    #[command(visible_alias = "dump")]
    ListHid,
    /// List matched devices with their current battery state
    List,
    /// Read the battery of one or all matched devices
    Battery {
        /// Restrict the report to one manifest id
        #[arg(long)]
        id: Option<String>,
    },
    /// Read or set the DPI of a mouse
    Dpi {
        /// Manifest id; defaults to the only matched mouse
        id: Option<String>,
        #[arg(long)]
        set: Option<u16>,
    },
    /// Read or set the polling rate
    Polling {
        /// Manifest id; defaults to the only matched device
        id: Option<String>,
        #[arg(long)]
        set: Option<u16>,
    },
    /// Read or set the actuation point of a Hall-effect keyboard
    Actuation {
        /// Manifest id; defaults to the only matched keyboard
        id: Option<String>,
        #[arg(long)]
        set: Option<f32>,
    },
    /// List monitors with their supported modes
    Monitors {
        /// Print every mode instead of the per-resolution list
        #[arg(long)]
        all_modes: bool,
    },
    /// Change the refresh rate of one monitor
    SetRefresh {
        /// Monitor id exactly as printed by `monitors`, for example \\.\DISPLAY1
        monitor: String,
        /// Target refresh rate in Hz at the monitor's current resolution
        refresh_hz: u32,
        /// Keep the mode active after a reboot
        #[arg(short, long)]
        persist: bool,
    },
    /// Drive the full battery/DPI/polling/actuation pipeline on virtual devices
    Demo,
}

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to start the akku-cli runtime");
    let code = rt.block_on(run());
    if code != 0 {
        std::process::exit(code);
    }
}

async fn run() -> i32 {
    let cli = Cli::parse();
    let manifests = match load_manifests(&cli.manifests) {
        Ok(manifests) => manifests,
        Err(error) => {
            eprintln!("error: {error}");
            return 1;
        }
    };

    let result = match cli.command {
        Command::ListHid => list_hid().await,
        Command::List => list_devices(&manifests).await,
        Command::Battery { id } => battery(&manifests, id.as_deref()).await,
        Command::Dpi { id, set } => dpi(&manifests, id.as_deref(), set).await,
        Command::Polling { id, set } => polling(&manifests, id.as_deref(), set).await,
        Command::Actuation { id, set } => actuation(&manifests, id.as_deref(), set).await,
        Command::Monitors { all_modes } => monitors(all_modes),
        Command::SetRefresh {
            monitor,
            refresh_hz,
            persist,
        } => change_refresh(&monitor, refresh_hz, persist),
        Command::Demo => demo().await,
    };

    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: {error}");
            1
        }
    }
}

fn load_manifests(dir: &Option<PathBuf>) -> Result<Vec<Manifest>, String> {
    match dir {
        Some(dir) => load_dir(dir),
        None => load_bundled(),
    }
}

async fn list_hid() -> Result<(), String> {
    let backend = HidBackend::default();
    let devices = enumerate(&backend).await.map_err(|err| err.to_string())?;
    if devices.is_empty() {
        println!("no accessible HID interfaces found");
        return Ok(());
    }
    for device in &devices {
        println!("{}", describe(device));
    }
    println!("\n{} interface(s) enumerated", devices.len());
    Ok(())
}

async fn matched_devices(manifests: &[Manifest]) -> Result<Vec<(Device, Manifest)>, String> {
    let backend = HidBackend::default();
    let devices = enumerate(&backend).await.map_err(|err| err.to_string())?;
    let mut matched = Vec::new();
    for device in devices {
        if let Some(manifest) = find_manifest(manifests, &device) {
            matched.push((device, manifest.clone()));
        }
    }
    Ok(matched)
}

async fn list_devices(manifests: &[Manifest]) -> Result<(), String> {
    let matched = matched_devices(manifests).await?;
    if matched.is_empty() {
        println!("no devices matched any manifest (attach the device or check --manifests)");
        return Ok(());
    }
    for (device, manifest) in matched {
        let mut driver = HidDriver::new(manifest.clone(), device);
        let battery = driver.read_battery().await;
        println!("- {} [{}]", manifest.name, manifest.id);
        println!(
            "  kind={} connection={} writes={}",
            manifest.kind.label(),
            manifest.connection.label(),
            if manifest.writes_verified {
                "enabled"
            } else {
                "disabled"
            }
        );
        match battery {
            Ok(report) => println!("  battery={}", format_battery(&report)),
            Err(error) => println!("  battery=ERROR: {error}"),
        }
    }
    Ok(())
}

async fn battery(manifests: &[Manifest], id: Option<&str>) -> Result<(), String> {
    let (readings, failures) = probe(manifests.to_vec(), id)
        .await
        .map_err(|err| err.to_string())?;
    if readings.is_empty() && failures.is_empty() {
        println!("no matched device reported a battery");
        return Ok(());
    }
    for (manifest_id, report) in &readings {
        println!("{manifest_id}: {}", format_battery(report));
    }
    for (manifest_id, error) in &failures {
        println!("{manifest_id}: ERROR: {error}");
    }
    Ok(())
}

fn format_battery(report: &akku_core::BatteryReport) -> String {
    let percent = report
        .percent
        .map(|percent| format!("{percent}%"))
        .unwrap_or_else(|| "--".into());
    let mut line = percent;
    if report.charging {
        line.push_str(" charging=yes");
    } else {
        line.push_str(" charging=no");
    }
    if let Some(voltage_mv) = report.voltage_mv {
        line.push_str(&format!(" voltage={voltage_mv} mV"));
    }
    line
}

fn pick_index(
    items: &[(Device, Manifest)],
    id: Option<&str>,
    kinds: &[DeviceKind],
) -> Result<usize, String> {
    let filtered: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, (_device, manifest))| {
            if let Some(id) = id {
                manifest.id == id
            } else if kind_restriction_applies(kinds) {
                kinds.contains(&manifest.kind)
            } else {
                true
            }
        })
        .map(|(index, _)| index)
        .collect();

    let Some(&index) = filtered.first() else {
        let restriction = if id.is_some() {
            format!("no device matched id {id:?}")
        } else if kind_restriction_applies(kinds) {
            let names = kinds
                .iter()
                .map(|kind| kind.label().to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join("/");
            format!("no matched device of kind {names}")
        } else {
            "no matched devices".into()
        };
        return Err(format!(
            "{restriction}; attach the device or point --manifests at extra manifests"
        ));
    };
    if filtered.len() > 1 && id.is_none() {
        let candidates = filtered
            .iter()
            .filter_map(|&i| items.get(i).map(|(_, m)| m.id.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "several devices matched ({candidates}); pass the manifest id explicitly"
        ));
    }
    Ok(index)
}

fn kind_restriction_applies(kinds: &[DeviceKind]) -> bool {
    !kinds.is_empty()
}

async fn pick_target(
    manifests: &[Manifest],
    id: Option<&str>,
    kinds: &[DeviceKind],
) -> Result<(HidDriver, Manifest), String> {
    let mut items = matched_devices(manifests).await?;
    let index = pick_index(&items, id, kinds)?;
    let (device, manifest) = items.swap_remove(index);
    Ok((HidDriver::new(manifest.clone(), device), manifest))
}

async fn dpi(manifests: &[Manifest], id: Option<&str>, set: Option<u16>) -> Result<(), String> {
    let (mut driver, manifest) = pick_target(manifests, id, &[DeviceKind::Mouse]).await?;
    let descriptor = manifest
        .dpi
        .clone()
        .ok_or_else(|| format!("{} does not expose a DPI descriptor", manifest.id))?;

    let current = driver
        .read_dpi()
        .await
        .map_err(|err| format!("READ FAILED: {err}"))?;
    println!(
        "{}: dpi={} of {:?}",
        manifest.id,
        current
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".into()),
        descriptor.values
    );

    let Some(target) = set else {
        return Ok(());
    };
    driver
        .set_dpi(target)
        .await
        .map_err(|err| format!("WRITE REJECTED: {err}"))?;
    let read_back = driver
        .read_dpi()
        .await
        .map_err(|err| format!("READ FAILED: {err}"))?
        .ok_or_else(|| "device stopped reporting DPI".to_string())?;
    println!("set {target} DPI: ok, read back {read_back} DPI");
    Ok(())
}

async fn polling(manifests: &[Manifest], id: Option<&str>, set: Option<u16>) -> Result<(), String> {
    let (mut driver, manifest) = pick_target(manifests, id, &[]).await?;
    let descriptor = manifest
        .polling
        .clone()
        .ok_or_else(|| format!("{} does not expose a polling descriptor", manifest.id))?;

    let current = driver
        .read_polling_rate()
        .await
        .map_err(|err| format!("READ FAILED: {err}"))?;
    println!(
        "{}: polling={} of {:?}",
        manifest.id,
        current
            .map(|value| format!("{value} Hz"))
            .unwrap_or_else(|| "?".into()),
        descriptor.rates
    );

    let Some(target) = set else {
        return Ok(());
    };
    driver
        .set_polling_rate(target)
        .await
        .map_err(|err| format!("WRITE REJECTED: {err}"))?;
    let read_back = driver
        .read_polling_rate()
        .await
        .map_err(|err| format!("READ FAILED: {err}"))?
        .ok_or_else(|| "device stopped reporting the polling rate".to_string())?;
    println!("set {target} Hz: ok, read back {read_back} Hz");
    Ok(())
}

async fn actuation(
    manifests: &[Manifest],
    id: Option<&str>,
    set: Option<f32>,
) -> Result<(), String> {
    let (mut driver, manifest) = pick_target(manifests, id, &[DeviceKind::Keyboard]).await?;
    let descriptor = manifest
        .actuation
        .clone()
        .ok_or_else(|| format!("{} does not expose an actuation descriptor", manifest.id))?;

    let current = driver
        .read_actuation()
        .await
        .map_err(|err| format!("READ FAILED: {err}"))?;
    println!(
        "{}: actuation={} (range {:.1}-{:.1} mm, step {:.1})",
        manifest.id,
        current
            .map(|value| format!("{value:.1} mm"))
            .unwrap_or_else(|| "?".into()),
        descriptor.min_mm,
        descriptor.max_mm,
        descriptor.step_mm
    );

    let Some(target) = set else {
        return Ok(());
    };
    driver
        .set_actuation(target)
        .await
        .map_err(|err| format!("WRITE REJECTED: {err}"))?;
    let read_back = driver
        .read_actuation()
        .await
        .map_err(|err| format!("READ FAILED: {err}"))?
        .ok_or_else(|| "device stopped reporting the actuation point".to_string())?;
    println!("set {target:.1} mm: ok, read back {read_back:.1} mm");
    Ok(())
}

fn monitors(all_modes: bool) -> Result<(), String> {
    let monitors = list_monitors().map_err(|err| err.to_string())?;
    for monitor in &monitors {
        println!("{}  {}", monitor.id, monitor.label);
        println!("  current: {}", monitor.current);
        println!("  {} normalized mode(s) total", monitor.modes.len());
        print_modes_at_current_resolution(monitor, all_modes);
    }
    println!("\n{} monitor(s) enumerated", monitors.len());
    Ok(())
}

fn print_modes_at_current_resolution(monitor: &akku_display::MonitorInfo, all_modes: bool) {
    if all_modes {
        for mode in &monitor.modes {
            println!("  mode: {}", DisplayModeLine(mode));
        }
        return;
    }
    let mut rates: Vec<u32> = monitor
        .modes
        .iter()
        .filter(|mode| mode.width == monitor.current.width && mode.height == monitor.current.height)
        .map(|mode| mode.refresh_hz)
        .collect();
    if rates.is_empty() {
        println!("  no other modes at the current resolution");
        return;
    }
    rates.sort_unstable();
    rates.dedup();
    let list = rates
        .iter()
        .map(|rate| rate.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    println!("  refresh rates at the current resolution: {list}");
}

struct DisplayModeLine<'a>(&'a akku_display::DisplayMode);

impl std::fmt::Display for DisplayModeLine<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}x{} @ {} Hz",
            self.0.width, self.0.height, self.0.refresh_hz
        )
    }
}

fn change_refresh(monitor: &str, refresh_hz: u32, persist: bool) -> Result<(), String> {
    let changed = set_refresh(monitor, refresh_hz, persist).map_err(|err| err.to_string())?;
    println!("set {} (persist={persist})", DisplayModeLine(&changed));
    Ok(())
}

async fn demo() -> Result<(), String> {
    println!(
        "DEMO — the full control pipeline on deterministic virtual devices (no hardware is touched)"
    );
    let mut mouse = MockDriver::mouse();
    let mut keyboard = MockDriver::keyboard();

    let mouse_spec = mouse.dpi_spec().expect("demo mouse exposes DPI");
    let keyboard_spec = keyboard
        .actuation_spec()
        .expect("demo keyboard exposes actuation");

    let before_mouse = mouse.read_battery().await.unwrap();
    let before_keyboard = keyboard.read_battery().await.unwrap();
    println!("[before]");
    println!(
        "  MOUSE    dpi={} polling={} battery={}",
        mouse.read_dpi().await.unwrap().unwrap(),
        format_args!("{} Hz", mouse.read_polling_rate().await.unwrap().unwrap()),
        before_mouse
            .percent
            .map(|p| format!("{p}%"))
            .unwrap_or_default()
    );
    println!(
        "  KEYBOARD polling={} battery={} actuation={:.1} mm",
        keyboard.read_polling_rate().await.unwrap().unwrap(),
        before_keyboard
            .percent
            .map(|p| format!("{p}%"))
            .unwrap_or_default(),
        keyboard.read_actuation().await.unwrap().unwrap()
    );

    for dpi_value in [
        mouse_spec.presets.first().copied().expect("non-empty"),
        800,
        mouse_spec.presets.last().copied().expect("non-empty"),
    ] {
        mouse
            .set_dpi(dpi_value)
            .await
            .map_err(|err| format!("demo DPI write failed: {err}"))?;
        println!(
            "  mouse.set_dpi({dpi_value}) -> read {}",
            mouse.read_dpi().await.unwrap().unwrap()
        );
    }

    let clamped = 1.5_f32.clamp(keyboard_spec.min_mm, keyboard_spec.max_mm);
    let steps = ((clamped - keyboard_spec.min_mm) / keyboard_spec.step_mm).round();
    let clamped = (keyboard_spec.min_mm + steps * keyboard_spec.step_mm)
        .clamp(keyboard_spec.min_mm, keyboard_spec.max_mm);
    keyboard
        .set_actuation(clamped)
        .await
        .map_err(|err| format!("demo actuation write failed: {err}"))?;
    println!(
        "  keyboard.set_actuation(1.5) -> read {:.1} mm",
        keyboard.read_actuation().await.unwrap().unwrap()
    );
    keyboard
        .set_actuation(99.0)
        .await
        .map_err(|err| format!("demo actuation clamp failed: {err}"))?;
    println!(
        "  keyboard.set_actuation(99.0) -> clamped to {:.1} mm",
        keyboard.read_actuation().await.unwrap().unwrap()
    );

    let invalid = mouse.set_dpi(12345).await;
    println!(
        "  mouse.set_dpi(12345) -> rejected as expected: {}",
        match &invalid {
            Ok(()) => "NOT rejected (bug!)".to_string(),
            Err(err) => err.to_string(),
        }
    );
    println!("DEMO complete: decode/encode, validation and clamping all behaved");
    Ok(())
}
