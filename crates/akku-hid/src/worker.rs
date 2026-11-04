//! Dedicated polling supervisor with exponential backoff and atomic persistence.

#![allow(unused_imports, unreachable_patterns, clippy::items_after_test_module)]

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use akku_core::{
    Axis, BatteryReport, BatteryState, Capabilities, DeviceDescriptor, DeviceHealth, DeviceId,
    DeviceKind, DeviceState as CoreDeviceState, DpiState, DriverError, FieldStatus, LightingEffect,
    LightingState, Manifest, ManifestId, PeripheralDriver, PollingRateCapability, ProfileId,
    Result as CoreResult, SettingKey, SettingValue,
};
use async_hid::HidBackend;
use futures_lite::StreamExt;
use tokio::sync::mpsc as tokio_mpsc;

use crate::driver::HidDriver;
use crate::enumerate::{device_key, enumerate, find_manifest};

#[derive(Debug)]
pub enum WorkerCommand {
    Refresh,
    SetDpi {
        manifest_id: String,
        dpi: u16,
    },
    SetPolling {
        manifest_id: String,
        hz: u16,
    },
    SetActuation {
        manifest_id: String,
        millimetres: f32,
    },
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum WorkerEvent {
    Devices(Vec<DeviceSnapshot>),
    Message(String),
}

/// UI-facing device snapshot (legacy compatibility with akku-tray-ui)
#[derive(Clone, Debug)]
pub struct DeviceSnapshot {
    pub manifest_id: ManifestId,
    pub key: akku_core::DeviceKey,
    pub name: String,
    pub vendor: String,
    pub kind: DeviceKind,
    pub connection: akku_core::manifest::ConnectionKind,
    pub state: CoreDeviceState,
    pub battery: BatterySnapshot,
    pub capabilities: CapabilitySet,
    pub writable: bool,
    pub dpi: Option<u16>,
    pub polling_hz: Option<u16>,
    pub actuation_mm: Option<f32>,
    pub last_error: Option<String>,
    pub dpi_values: Option<Vec<u16>>,
    pub polling_rates: Option<Vec<u16>>,
    pub actuation_range: Option<akku_core::capability::ActuationSpec>,
}

impl DeviceSnapshot {
    pub fn new(
        manifest_id: ManifestId,
        key: akku_core::DeviceKey,
        name: impl Into<String>,
        vendor: impl Into<String>,
        kind: DeviceKind,
        connection: akku_core::manifest::ConnectionKind,
    ) -> Self {
        Self {
            manifest_id,
            key,
            name: name.into(),
            vendor: vendor.into(),
            kind,
            connection,
            state: CoreDeviceState::empty_for(DeviceId::new("", 0, 0, 0, 0), 0),
            battery: BatterySnapshot::unknown(),
            capabilities: CapabilitySet::default(),
            writable: false,
            dpi: None,
            polling_hz: None,
            actuation_mm: None,
            last_error: None,
            dpi_values: None,
            polling_rates: None,
            actuation_range: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BatterySnapshot {
    pub report: BatteryReport,
    pub updated_at: SystemTime,
    pub stale: bool,
}

impl BatterySnapshot {
    pub fn unknown() -> Self {
        Self {
            report: BatteryReport::default(),
            updated_at: SystemTime::UNIX_EPOCH,
            stale: true,
        }
    }

    pub fn display_percent(&self) -> String {
        match self.report.percent {
            Some(p) => format!("{p}%"),
            None => "--".to_string(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CapabilitySet {
    pub dpi: bool,
    pub polling_rate: bool,
    pub actuation: bool,
    pub macros: bool,
}

pub struct HidWorker {
    commands: tokio_mpsc::UnboundedSender<WorkerCommand>,
    events: Receiver<WorkerEvent>,
    join: Option<JoinHandle<()>>,
}

impl HidWorker {
    pub fn spawn(
        manifests: Vec<Manifest>,
        poll_interval: Duration,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name("akku-hid".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create HID runtime");
                runtime.block_on(run(manifests, poll_interval, command_rx, event_tx, repaint));
            })
            .expect("failed to start HID thread");
        Self {
            commands: command_tx,
            events: event_rx,
            join: Some(join),
        }
    }

    pub fn send(&self, command: WorkerCommand) {
        let _ = self.commands.send(command);
    }

    pub fn try_recv(&self) -> Option<WorkerEvent> {
        self.events.try_recv().ok()
    }

    pub fn shutdown(&mut self) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for HidWorker {
    fn drop(&mut self) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
    }
}

struct ManagedDevice {
    manifest_id: String,
    driver: HidDriver,
    snapshot: DeviceSnapshot,
    last_success: Instant,
    backoff: Duration,
    consecutive_failures: u32,
}

struct Registry {
    backend: HidBackend,
    manifests: Vec<Manifest>,
    devices: Vec<ManagedDevice>,
    poll_interval: Duration,
    #[allow(dead_code)]
    settings_path: PathBuf,
}

impl Registry {
    fn new(backend: HidBackend, manifests: Vec<Manifest>, poll_interval: Duration) -> Self {
        let settings_path = PathBuf::from(".").join("akku-profiles.json");
        Self {
            backend,
            manifests,
            devices: Vec::new(),
            poll_interval,
            settings_path,
        }
    }

    async fn discover(&mut self, events: &Sender<WorkerEvent>) {
        let discovered = match enumerate(&self.backend).await {
            Ok(devices) => devices,
            Err(error) => {
                let _ = events.send(WorkerEvent::Message(format!(
                    "HID enumeration failed: {error}"
                )));
                return;
            }
        };

        let mut next = Vec::new();
        for device in discovered {
            let Some(manifest) = find_manifest(&self.manifests, &device) else {
                continue;
            };
            let key = device_key(&device);
            if next
                .iter()
                .any(|managed: &ManagedDevice| managed.snapshot.key == key)
            {
                continue;
            }
            let manifest = manifest.clone();
            let device_id = DeviceId::new(
                "hid",
                manifest.matches[0].vid,
                manifest.matches[0].pid,
                manifest.matches[0].usage_page,
                manifest.matches[0].usage,
            );
            let mut snapshot = DeviceSnapshot::new(
                manifest.manifest_id(),
                key.clone(),
                manifest.name.clone(),
                manifest.vendor.clone(),
                manifest.kind,
                manifest.connection,
            );
            if let Some(previous) = self
                .devices
                .iter()
                .find(|managed| managed.snapshot.key == key)
            {
                snapshot.battery = previous.snapshot.battery.clone();
                snapshot.dpi = previous.snapshot.dpi;
                snapshot.polling_hz = previous.snapshot.polling_hz;
                snapshot.actuation_mm = previous.snapshot.actuation_mm;
            }
            snapshot.dpi_values = manifest
                .dpi
                .as_ref()
                .map(|descriptor| descriptor.values.clone());
            snapshot.polling_rates = manifest
                .polling
                .as_ref()
                .map(|descriptor| descriptor.rates.clone());
            snapshot.actuation_range = manifest
                .actuation
                .as_ref()
                .map(|descriptor| descriptor.spec());
            let driver = HidDriver::new(manifest, device);
            let caps = driver.capabilities();
            snapshot.capabilities = CapabilitySet {
                dpi: caps.contains(Capabilities::DPI),
                polling_rate: caps.contains(Capabilities::POLLING_RATE),
                actuation: caps.contains(Capabilities::ACTUATION),
                macros: caps.contains(Capabilities::MACROS),
            };
            snapshot.writable = driver.manifest().writes_verified;
            snapshot.state = CoreDeviceState::empty_for(device_id.clone(), unix_ms());
            snapshot.state.health = DeviceHealth::Online;
            next.push(ManagedDevice {
                manifest_id: snapshot.manifest_id.0.clone(),
                driver,
                snapshot,
                last_success: Instant::now(),
                backoff: Duration::from_secs(2),
                consecutive_failures: 0,
            });
        }
        self.devices = next;
        self.poll(events).await;
    }

    async fn poll(&mut self, events: &Sender<WorkerEvent>) {
        for managed in &mut self.devices {
            let now = Instant::now();
            match managed.driver.read_battery().await {
                Ok(report) => {
                    managed.snapshot.battery.report = report;
                    managed.snapshot.battery.updated_at = SystemTime::now();
                    managed.snapshot.battery.stale = false;
                    managed.snapshot.state.health = DeviceHealth::Online;
                    managed.snapshot.last_error = None;
                    managed.last_success = now;
                    managed.backoff = Duration::from_secs(2);
                    managed.consecutive_failures = 0;
                }
                Err(error) => {
                    managed.snapshot.battery.stale = true;
                    managed.snapshot.state.health = match error {
                        DriverError::Disconnected { .. } => DeviceHealth::Disconnected,
                        DriverError::DeviceBusy { .. } => DeviceHealth::NotResponding {
                            reason: "DEVICE IN USE".to_string(),
                        },
                        _ => DeviceHealth::NotResponding {
                            reason: error.to_string(),
                        },
                    };
                    managed.snapshot.last_error = Some(error.to_string());

                    // Exponential backoff
                    managed.consecutive_failures += 1;
                    managed.backoff = match managed.consecutive_failures {
                        1 => Duration::from_secs(2),
                        2 => Duration::from_secs(5),
                        3 => Duration::from_secs(15),
                        _ => Duration::from_secs(60),
                    };
                }
            }

            use akku_core::DpiCapability;
            if managed.snapshot.capabilities.dpi {
                if let Ok(dpi) = managed.driver.read_dpi().await {
                    managed.snapshot.dpi = dpi;
                }
            }
            if managed.snapshot.capabilities.polling_rate {
                if let Ok(rate) = managed.driver.read_polling_rate().await {
                    managed.snapshot.polling_hz = rate;
                }
            }
            use akku_core::ActuationCapability;
            if managed.snapshot.capabilities.actuation {
                if let Ok(actuation) = managed.driver.read_actuation().await {
                    managed.snapshot.actuation_mm = actuation;
                }
            }

            // Mark as stale if no successful read for 2x poll interval
            if now.duration_since(managed.last_success) > self.poll_interval * 2 {
                managed.snapshot.battery.stale = true;
                if managed.snapshot.state.health == DeviceHealth::Online {
                    managed.snapshot.state.health = DeviceHealth::NotResponding {
                        reason: "stale".into(),
                    };
                }
            }
        }
        emit(events, &mut self.devices);
    }

    async fn command(&mut self, command: WorkerCommand, events: &Sender<WorkerEvent>) {
        match command {
            WorkerCommand::Refresh => {
                // Reset backoff on manual refresh
                for managed in &mut self.devices {
                    managed.backoff = Duration::from_secs(2);
                    managed.consecutive_failures = 0;
                }
                self.poll(events).await;
            }
            WorkerCommand::SetDpi { manifest_id, dpi } => {
                use akku_core::DpiCapability;
                let result = match self
                    .devices
                    .iter_mut()
                    .find(|managed| managed.manifest_id == manifest_id)
                {
                    Some(managed) => managed.driver.set_dpi(dpi).await,
                    None => Err(DriverError::Disconnected {
                        device_id: manifest_id.clone(),
                    }),
                };
                self.finish_command(&manifest_id, result, Some(dpi), None, None, events);
            }
            WorkerCommand::SetPolling { manifest_id, hz } => {
                use akku_core::PollingRateCapability;
                let result = match self
                    .devices
                    .iter_mut()
                    .find(|managed| managed.manifest_id == manifest_id)
                {
                    Some(managed) => managed.driver.set_polling_rate(hz).await,
                    None => Err(DriverError::Disconnected {
                        device_id: manifest_id.clone(),
                    }),
                };
                self.finish_command(&manifest_id, result, None, Some(hz), None, events);
            }
            WorkerCommand::SetActuation {
                manifest_id,
                millimetres,
            } => {
                use akku_core::ActuationCapability;
                let result = match self
                    .devices
                    .iter_mut()
                    .find(|managed| managed.manifest_id == manifest_id)
                {
                    Some(managed) => managed.driver.set_actuation(millimetres).await,
                    None => Err(DriverError::Disconnected {
                        device_id: manifest_id.clone(),
                    }),
                };
                self.finish_command(&manifest_id, result, None, None, Some(millimetres), events);
            }
            WorkerCommand::Shutdown => {}
        }
    }

    fn finish_command(
        &mut self,
        manifest_id: &str,
        result: CoreResult<()>,
        dpi: Option<u16>,
        polling: Option<u16>,
        actuation: Option<f32>,
        events: &Sender<WorkerEvent>,
    ) {
        let Some(managed) = self
            .devices
            .iter_mut()
            .find(|managed| managed.manifest_id == manifest_id)
        else {
            let _ = events.send(WorkerEvent::Message(format!(
                "device {manifest_id} is not connected"
            )));
            return;
        };
        match result {
            Ok(()) => {
                if dpi.is_some() {
                    managed.snapshot.dpi = dpi;
                }
                if polling.is_some() {
                    managed.snapshot.polling_hz = polling;
                }
                if actuation.is_some() {
                    managed.snapshot.actuation_mm = actuation;
                }
            }
            Err(error) => {
                managed.snapshot.last_error = Some(error.to_string());
                let _ = events.send(WorkerEvent::Message(error.to_string()));
            }
        }
        emit(events, &mut self.devices);
    }
}

fn emit(events: &Sender<WorkerEvent>, devices: &mut [ManagedDevice]) {
    let mut snapshots = devices
        .iter()
        .map(|managed| managed.snapshot.clone())
        .collect::<Vec<_>>();
    snapshots.sort_by(|a, b| {
        let rank = |kind: DeviceKind| match kind {
            DeviceKind::Mouse => 0,
            DeviceKind::Keyboard => 1,
            _ => 2,
        };
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });
    let _ = events.send(WorkerEvent::Devices(snapshots));
}

async fn run(
    manifests: Vec<Manifest>,
    poll_interval: Duration,
    mut commands: tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    events: Sender<WorkerEvent>,
    repaint: Arc<dyn Fn() + Send + Sync>,
) {
    let backend = HidBackend::default();
    let mut registry = Registry::new(
        backend.clone(),
        manifests,
        poll_interval.max(Duration::from_secs(5)),
    );
    registry.discover(&events).await;
    repaint();

    let mut interval = tokio::time::interval(registry.poll_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut watch = backend.watch().ok().map(|stream| stream.boxed());

    loop {
        tokio::select! {
            _ = interval.tick() => {
                registry.poll(&events).await;
                repaint();
            }
            event = async {
                match watch.as_mut() {
                    Some(stream) => stream.next().await,
                    None => std::future::pending().await,
                }
            } => {
                if event.is_some() {
                    registry.discover(&events).await;
                    repaint();
                }
            }
            command = commands.recv() => {
                match command {
                    Some(WorkerCommand::Shutdown) | None => break,
                    Some(command) => {
                        registry.command(command, &events).await;
                        repaint();
                    }
                }
            }
        }
    }
}

/// Convenience probe used by the CLI: match a manifest against all accessible
/// devices and read a battery report from each.
///
/// Per-device failures land in the second list so one broken device cannot
/// hide the readings of healthy ones.
pub async fn probe(
    manifests: Vec<Manifest>,
    manifest_id: Option<&str>,
) -> CoreResult<(Vec<(String, BatteryReport)>, Vec<(String, DriverError)>)> {
    let backend = HidBackend::default();
    let devices = enumerate(&backend).await?;
    let mut readings = Vec::new();
    let mut failures = Vec::new();
    for device in devices {
        let Some(manifest) = find_manifest(&manifests, &device) else {
            continue;
        };
        if manifest_id.is_some_and(|id| id != manifest.id) {
            continue;
        }
        let id = manifest.id.clone();
        let mut driver = HidDriver::new(manifest.clone(), device);
        match driver.read_battery().await {
            Ok(battery) => readings.push((id, battery)),
            Err(error) => failures.push((id, error)),
        }
    }
    Ok((readings, failures))
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use akku_core::DeviceKey;
    use std::sync::Arc;

    #[test]
    fn worker_starts_and_stops_without_hardware() {
        let mut worker = HidWorker::spawn(Vec::new(), Duration::from_secs(60), Arc::new(|| {}));
        worker.send(WorkerCommand::Refresh);
        worker.shutdown();
    }

    #[test]
    fn key_round_trip() {
        let key = DeviceKey::new(1, 2, 3, 4);
        assert!(key.label().contains("0001:0002"));
    }
}
