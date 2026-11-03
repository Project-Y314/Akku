# Akku

![Akku logo](assets/akku.png)

**Akku** (German for *battery*) — a tiny tray tool that shows battery levels for your wireless mouse, keyboard, and other devices. Click the tray icon to see details and adjust DPI, polling rate, or actuation point.

## Quick start

```powershell
# Build & run (release)
cargo build --release
.\target\release\akku.exe

# Start hidden (for autostart)
.\target\release\akku.exe --hidden

# Demo mode — no hardware needed
.\target\release\akku.exe --demo
```

## What you get

- **Tray icon** — shows battery % for all connected devices; hover for details
- **Click tray** — opens a compact window with per-device cards
- **Per device** — battery, DPI (presets + custom), polling rate (Hz), actuation (mm)
- **Monitors** — quick refresh-rate switching
- **Settings** — poll interval, dark/light theme, Windows autostart

## Supported devices (built-in)

| Device | VID:PID | Connection | Battery | DPI | Polling | Actuation |
|--------|---------|------------|---------|-----|---------|-----------|
| Attack Shark mouse | 1D57:FA09 / 25A7:FA09 | USB receiver | ✅ | ✅ | ✅ | — |
| Epomaker P65 keyboard | 320F:5055 | USB receiver | ✅ | — | ✅ | ✅ |

> **Note:** Writes (DPI, polling, actuation) are read-only until a manifest has `writes_verified = true`. Demo mode works fully.

## CLI

```powershell
akku-cli.exe list-hid        # all HID interfaces
akku-cli.exe list            # matched devices + battery
akku-cli.exe dpi --set 800 attack-shark
akku-cli.exe polling --set 1000
akku-cli.exe actuation --set 1.5
akku-cli.exe monitors
akku-cli.exe set-refresh \\.\DISPLAY1 144 --persist
akku-cli.exe demo
```

## Build

- Rust 1.99+ (pinned via `rust-toolchain.toml`)
- Windows 10/11

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
cargo build --release
```

## License

MIT — see [LICENSE](LICENSE).