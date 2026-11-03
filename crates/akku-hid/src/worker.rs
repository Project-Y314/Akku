use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use akku_core::{
    ActuationCapability, DeviceKind, DeviceSnapshot, DeviceState, DriverError, DpiCapability,
    Manifest, PeripheralDriver, PollingRateCapability,
};
use async_hid::HidBackend;
use futures_lite::StreamExt;
use tokio::sync::mpsc as tokio_mpsc;

use crate::driver::HidDriver;
use crate::enumerate::{device_key, enumerate, find_manifest};

#[derive(Debug)]
pub enum WorkerCommand {
    Refresh,
    SetDpi { manifest_id: String, dpi: u16 },
    SetPolling { manifest_id: String, hz: u16 },
    SetActuation { manifest_id: String, millimetres: f32 },
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum WorkerEvent {
    Devices(Vec<DeviceSnapshot>),
    Message(String),
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
                runtime.block_on(run(
                    manifests,
                    poll_interval,
                    command_rx,
                    event_tx,
                    repaint,
                ));
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
}

struct Registry {
    backend: HidBackend,
    manifests: Vec<Manifest>,
    devices: Vec<ManagedDevice>,
    poll_interval: Duration,
}

impl Registry {
    fn new(backend: HidBackend, manifests: Vec<Manifest>, poll_interval: Duration) -> Self {
        Self {
            backend,
            manifests,
            devices: Vec::new(),
            poll_interval,
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
            let driver = HidDriver::new(manifest, device);
            snapshot.capabilities = driver.capabilities();
            snapshot.writable = driver.manifest().writes_verified;
            snapshot.state = DeviceState::Connected;
            next.push(ManagedDevice {
                manifest_id: snapshot.manifest_id.0.clone(),
                driver,
                snapshot,
                last_success: Instant::now(),
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
                    managed.snapshot.state = DeviceState::Connected;
                    managed.snapshot.last_error = None;
                    managed.last_success = now;
                }
                Err(error) => {
                    managed.snapshot.battery.stale = true;
                    managed.snapshot.state = match error {
                        DriverError::Disconnected => DeviceState::Disconnected,
                        DriverError::DeviceBusy => {
                            DeviceState::Error("DEVICE IN USE".to_string())
                        }
                        _ => DeviceState::Stale,
                    };
                    managed.snapshot.last_error = Some(error.to_string());
                }
            }

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
            if managed.snapshot.capabilities.actuation {
                if let Ok(actuation) = managed.driver.read_actuation().await {
                    managed.snapshot.actuation_mm = actuation;
                }
            }

            if now.duration_since(managed.last_success) > self.poll_interval * 2 {
                managed.snapshot.battery.stale = true;
                if managed.snapshot.state == DeviceState::Connected {
                    managed.snapshot.state = DeviceState::Stale;
                }
            }
        }
        emit(events, &mut self.devices);
    }

    async fn command(&mut self, command: WorkerCommand, events: &Sender<WorkerEvent>) {
        match command {
            WorkerCommand::Refresh => self.poll(events).await,
            WorkerCommand::SetDpi { manifest_id, dpi } => {
                let result = match self
                    .devices
                    .iter_mut()
                    .find(|managed| managed.manifest_id == manifest_id)
                {
                    Some(managed) => managed.driver.set_dpi(dpi).await,
                    None => Err(DriverError::Disconnected),
                };
                self.finish_command(&manifest_id, result, Some(dpi), None, None, events);
            }
            WorkerCommand::SetPolling { manifest_id, hz } => {
                let result = match self
                    .devices
                    .iter_mut()
                    .find(|managed| managed.manifest_id == manifest_id)
                {
                    Some(managed) => managed.driver.set_polling_rate(hz).await,
                    None => Err(DriverError::Disconnected),
                };
                self.finish_command(&manifest_id, result, None, Some(hz), None, events);
            }
            WorkerCommand::SetActuation {
                manifest_id,
                millimetres,
            } => {
                let result = match self
                    .devices
                    .iter_mut()
                    .find(|managed| managed.manifest_id == manifest_id)
                {
                    Some(managed) => managed.driver.set_actuation(millimetres).await,
                    None => Err(DriverError::Disconnected),
                };
                self.finish_command(
                    &manifest_id,
                    result,
                    None,
                    None,
                    Some(millimetres),
                    events,
                );
            }
            WorkerCommand::Shutdown => {}
        }
    }

    fn finish_command(
        &mut self,
        manifest_id: &str,
        result: akku_core::Result<()>,
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
) -> Result<(Vec<(String, akku_core::BatteryReport)>, Vec<(String, DriverError)>), DriverError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use akku_core::DeviceKey;
    use std::sync::Arc;

    #[test]
    fn worker_starts_and_stops_without_hardware() {
        let mut worker = HidWorker::spawn(
            Vec::new(),
            Duration::from_secs(60),
            Arc::new(|| {}),
        );
        worker.send(WorkerCommand::Refresh);
        worker.shutdown();
    }

    #[test]
    fn key_round_trip() {
        let key = DeviceKey::new(1, 2, 3, 4);
        assert!(key.label().contains("0001:0002"));
    }
}
