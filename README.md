# Akku

**Akku** (German for *battery*) is a local-first control panel for wireless
gaming peripherals. It unifies what each vendor app does separately — battery
status, DPI stages, polling rate, keyboard actuation points and monitor
refresh rate — behind one small, fast, dependency-light binary set.

```
┌────────────┐   HID feature reports   ┌─────────────┐
│  devices   │◄───────────────────────►│  akku-core  │  manifests + settings
└────────────┘        akku-hid         └─────────────┘
                                             │
                        ┌────────────────────┴─────────────┐
                        ▼                                  ▼
                 akku (tray UI)                        akku-cli
        battery tray icon + panel                terminal inspection
        DPI / polling / actuation / Hz           + writes + demo mode
```

## Components

| Crate           | Binary      | Purpose                                          |
| --------------- | ----------- | ------------------------------------------------ |
| `akku-core`     | —           | Device manifests, snapshots, settings, discharge tracker |
| `akku-hid`      | —           | Async HID worker (`async-hid`), report decode, mock driver |
| `akku-display`  | —           | Monitor enumeration and refresh-rate switching   |
| `akku-tray-ui`  | `akku.exe`  | eframe tray application with battery tray icon   |
| `akku-cli`      | `akku-cli.exe` | Scriptable CLI over the same pipeline         |

## Supported devices

Device support is data-driven: a [`devices/*.toml`](devices) manifest describes
VID/PID/usage matches plus the battery/DPI/polling/actuation report layout.
Bundled today:

- **Attack Shark** mouse (`1D57:FA09`, `25A7:FA09` — Beken / GearHub receivers)
- **Epomaker P65** keyboard

### `writes_verified` policy

Manifests carry a `writes_verified` flag. **Both bundled manifests currently
set `writes_verified = false`**, which means:

- Reading battery/DPI/polling/actuation is allowed.
- Every write path is **deliberately rejected** at the driver level with an
  explanatory error.

This is intentional, not a bug: write commands are only unlocked once a real
hardware capture confirms the feature-report layout, byte offsets and
acknowledgement behaviour. Flip the flag to `true` after capturing genuine
set-report traffic from the vendor driver (and update `notes` with your
findings). The demo mode exists precisely so the remaining pipeline — UI,
wiring, feedback — can be exercised without hardware.

## Build

Requirements: Rust ≥ 1.99 (`rust-toolchain.toml` pins the toolchain), Windows
10/11 (HID + display + registry features are Windows-bound).

```powershell
cargo build --release
# binaries land in target\release:
#   akku.exe      — tray application
#   akku-cli.exe  — command line
```

## Run

```powershell
# tray application against real devices
target\release\akku.exe

# start hidden in the tray (the autostart entry uses this)
target\release\akku.exe --hidden

# drive the full UI on deterministic virtual devices, no hardware needed
target\release\akku.exe --demo
```

```powershell
# CLI examples
target\release\akku-cli.exe list-hid        # every accessible HID interface
target\release\akku-cli.exe list            # matched devices + battery state
target\release\akku-cli.exe battery         # battery of one or all devices
target\release\akku-cli.exe dpi             # read DPI (write: add --set <value>)
target\release\akku-cli.exe dpi --set 800 attack-shark
target\release\akku-cli.exe polling --set 1000
target\release\akku-cli.exe actuation --set 1.5
target\release\akku-cli.exe monitors        # enumerate monitors + modes
target\release\akku-cli.exe set-refresh \\.\DISPLAY1 144 --persist
target\release\akku-cli.exe demo            # full pipeline on virtual devices
```

`akku-cli --help` and `akku-cli <command> --help` document every flag, including
`--manifests <dir>` for loading extra `.toml` manifests that override bundled
ones by id.

## Tray application

- Battery tray icon per connected device, tooltip with percentages, right-click
  menu (show window / refresh / quit).
- Close-to-tray: closing the window hides it; **Quit** in the tray menu exits.
- Window panels: devices (battery, connection, DPI/polling/actuation controls),
  monitors (per-monitor refresh rate with optional persistence across reboots)
  and settings (poll interval, theme, autostart writing
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
- Autostart launches `akku.exe --hidden`.

## Development

```powershell
cargo test --workspace       # unit tests (23 tests)
cargo clippy --workspace --all-targets   # zero-warning gate
cargo fmt --all
```

Settings persist to `%APPDATA%\Akku\settings.toml` (atomic write).

## License

MIT — see [LICENSE](LICENSE).
