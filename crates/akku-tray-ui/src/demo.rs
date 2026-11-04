//! A thread that drives the identical command/event interface as the real
//! [`akku_hid::HidWorker`], but on deterministic virtual [`MockDriver`]s.
//!
//! Usage: `akku --demo`. The battery drains slowly so the UI is never static;
//! writes validate and clamp exactly like the real path.

#![allow(unused_imports)]

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use akku_core::{
    ActuationCapability, Capabilities, ConnectionKind, DeviceId, DeviceKind, DeviceState,
    DpiCapability, ManifestId, PeripheralDriver, PollingRateCapability,
};
use akku_hid::{CapabilitySet, DeviceSnapshot, MockDriver, WorkerCommand, WorkerEvent};
use tokio::sync::mpsc as tokio_mpsc;

const DEMO_POLL_INTERVAL: Duration = Duration::from_secs(5);
const MOUSE_ID: &str = "attack-shark";
const KEYBOARD_ID: &str = "epomaker-p65";

pub struct DemoWorker {
    commands: tokio_mpsc::UnboundedSender<WorkerCommand>,
    events: Receiver<WorkerEvent>,
    join: Option<JoinHandle<()>>,
}

impl DemoWorker {
    pub fn spawn() -> Self {
        let (commands, command_rx) = tokio_mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("akku-demo".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create demo runtime");
                runtime.block_on(run(command_rx, event_tx));
            })
            .expect("failed to start demo thread");
        Self {
            commands,
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

impl Drop for DemoWorker {
    fn drop(&mut self) {
        let _ = self.commands.send(WorkerCommand::Shutdown);
    }
}

/// One interface for the real HID pipeline and the demo pipeline.
pub enum Runtime {
    Real(akku_hid::HidWorker),
    Demo(DemoWorker),
}

impl Runtime {
    pub fn send(&self, command: WorkerCommand) {
        match self {
            Self::Real(worker) => worker.send(command),
            Self::Demo(worker) => worker.send(command),
        }
    }

    pub fn try_recv(&self) -> Option<WorkerEvent> {
        match self {
            Self::Real(worker) => worker.try_recv(),
            Self::Demo(worker) => worker.try_recv(),
        }
    }

    pub fn shutdown(&mut self) {
        match self {
            Self::Real(worker) => worker.shutdown(),
            Self::Demo(worker) => worker.shutdown(),
        }
    }
}

struct MouseTrack {
    percent: u8,
    driver: MockDriver,
}

struct KeyboardTrack {
    driver: MockDriver,
}

struct DemoState {
    mouse: MouseTrack,
    keyboard: KeyboardTrack,
}

impl DemoState {
    fn initial() -> Self {
        Self {
            mouse: MouseTrack {
                percent: 87,
                driver: MockDriver::mouse(),
            },
            keyboard: KeyboardTrack {
                driver: MockDriver::keyboard(),
            },
        }
    }

    async fn snapshots(&mut self) -> Vec<DeviceSnapshot> {
        let mut out = Vec::new();
        let mouse_snapshot = snapshot(
            "Attack Shark (demo)",
            "Attack Shark",
            MOUSE_ID,
            akku_core::DeviceKind::Mouse,
            &mut self.mouse.driver,
        )
        .await;
        out.push(mouse_snapshot);
        let keyboard_snapshot = snapshot(
            "Epomaker P65 (demo)",
            "Epomaker",
            KEYBOARD_ID,
            akku_core::DeviceKind::Keyboard,
            &mut self.keyboard.driver,
        )
        .await;
        out.push(keyboard_snapshot);
        out
    }

    /// Apply one command, returning an optional human-readable status line.
    async fn apply(&mut self, command: WorkerCommand) -> Option<String> {
        match command {
            WorkerCommand::Refresh => Some("demo: refreshed".into()),
            WorkerCommand::Shutdown => None,
            WorkerCommand::SetDpi { manifest_id, dpi } => {
                if manifest_id != MOUSE_ID {
                    return Some("demo: unexpected DPI target".into());
                }
                match self.mouse.driver.set_dpi(dpi).await {
                    Ok(()) => Some(format!("demo: mouse DPI set to {dpi}")),
                    Err(error) => Some(format!("demo: {error}")),
                }
            }
            WorkerCommand::SetPolling { manifest_id, hz } => {
                let verdict = match manifest_id.as_str() {
                    MOUSE_ID => self.mouse.driver.set_polling_rate(hz).await,
                    KEYBOARD_ID => self.keyboard.driver.set_polling_rate(hz).await,
                    _ => return Some("demo: unexpected polling target".into()),
                };
                match verdict {
                    Ok(()) => Some(format!("demo: polling rate set to {hz} Hz")),
                    Err(error) => Some(format!("demo: {error}")),
                }
            }
            WorkerCommand::SetActuation {
                manifest_id,
                millimetres,
            } => {
                if manifest_id != KEYBOARD_ID {
                    return Some("demo: unexpected actuation target".into());
                }
                match self.keyboard.driver.set_actuation(millimetres).await {
                    Ok(()) => Some(format!("demo: actuation set to {millimetres:.1} mm")),
                    Err(error) => Some(format!("demo: {error}")),
                }
            }
        }
    }

    fn drain_battery(&mut self) {
        self.mouse.percent = self.mouse.percent.saturating_sub(1).max(20);
        self.mouse.driver = MockDriver::mouse().with_battery(self.mouse.percent);
    }
}

async fn snapshot(
    name: &str,
    vendor: &str,
    manifest_id: &str,
    kind: DeviceKind,
    driver: &mut MockDriver,
) -> DeviceSnapshot {
    let key = driver.key().clone();
    let caps = driver.capabilities();
    let mut snapshot = DeviceSnapshot::new(
        ManifestId(manifest_id.to_string()),
        key,
        name.to_string(),
        vendor.to_string(),
        kind,
        ConnectionKind::UsbReceiver,
    );
    snapshot.capabilities = CapabilitySet {
        dpi: caps.contains(Capabilities::DPI),
        polling_rate: caps.contains(Capabilities::POLLING_RATE),
        actuation: caps.contains(Capabilities::ACTUATION),
        macros: caps.contains(Capabilities::MACROS),
    };
    snapshot.writable = true;
    snapshot.state = akku_core::DeviceState::empty_for(DeviceId::new("", 0, 0, 0, 0), 0);
    snapshot.state.health = akku_core::DeviceHealth::Online;
    snapshot.last_error = None;

    if let Ok(report) = driver.read_battery().await {
        snapshot.battery.report = report;
    }
    snapshot.battery.updated_at = SystemTime::now();
    snapshot.battery.stale = false;

    snapshot.dpi = driver.read_dpi().await.ok().flatten();
    snapshot.polling_hz = driver.read_polling_rate().await.ok().flatten();
    snapshot.actuation_mm = driver.read_actuation().await.ok().flatten();
    snapshot.dpi_values = driver.dpi_spec().map(|spec| spec.presets);
    snapshot.polling_rates = driver.polling_spec().map(|spec| spec.rates_hz);
    snapshot.actuation_range = driver.actuation_spec();
    snapshot
}

async fn run(
    mut commands: tokio_mpsc::UnboundedReceiver<WorkerCommand>,
    events: Sender<WorkerEvent>,
) {
    let mut state = DemoState::initial();
    let mut emit_at = Instant::now();
    events
        .send(WorkerEvent::Devices(state.snapshots().await))
        .ok();

    loop {
        tokio::time::sleep(Duration::from_millis(120)).await;

        loop {
            let verdict = match commands.try_recv() {
                Ok(WorkerCommand::Shutdown) => return,
                Err(tokio_mpsc::error::TryRecvError::Empty) => break,
                Err(tokio_mpsc::error::TryRecvError::Disconnected) => return,
                Ok(command) => state.apply(command).await,
            };
            if let Some(message) = verdict {
                events.send(WorkerEvent::Message(message)).ok();
            }
            events
                .send(WorkerEvent::Devices(state.snapshots().await))
                .ok();
        }

        if emit_at.elapsed() >= DEMO_POLL_INTERVAL {
            emit_at = Instant::now();
            state.drain_battery();
            events
                .send(WorkerEvent::Devices(state.snapshots().await))
                .ok();
        }
    }
}
