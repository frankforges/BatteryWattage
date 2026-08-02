# Wattage Monitor

A Windows system tray icon showing real-time battery charge/discharge wattage, drawn as a live number.

Reads the battery driver directly over `DeviceIoControl` — no WMI, no PowerShell subprocess, no polling a shell every second. The release binary is **~136 KB** and idles at a few MB of RAM.

## What it shows

| Tray label | Meaning |
|---|---|
| `18` | Charging at 18 W |
| `-12` | Discharging at 12 W |
| `T` | Charge **threshold** — AC connected and the battery is deliberately held, e.g. Lenovo Conservation Mode. Also shown for any reading under ±1 W on AC, since that is a hold rather than real charge/discharge. |
| `N/A` | No battery detected at all, or no reading for 10 seconds |

Hovering shows a tooltip with the rate in watts and amps, terminal voltage, and charge percentage.

The colour follows state: green charging, amber discharging, blue-grey idle (in the default preset).

## Why `T` and not a number

`ChargeRate` from the battery driver is **net flow into the cells**, not adapter output:

```
adapter total = system consumption + ChargeRate
```

When a charge threshold holds the battery at, say, 80%, net flow sits at ~0 W while the adapter is still delivering real power to run the machine. Rendering that as `0` implies "nothing is happening", which is wrong. `T` says "deliberately held".

Readings under ±1 W are folded into `T` for the same reason — a battery hovering at 0.4 W is being held, not charged, and letting it flicker between `0` and `1` is noise.

## Menu

Right-click the tray icon:

- **Colour presets** — Default, All White, All Green, High Contrast
- **Background pill** — toggle the rounded background behind the number
- **Text scale** — 75% / 100% / 125% / 150% / 200%
- **Run at startup** — adds/removes an `HKCU\...\Run` entry
- **About**, **Exit**

Settings persist to `HKCU\Software\WattageMonitor`.

## Build

Requires the Rust toolchain and the MSVC target.

```powershell
cargo build --release
```

The binary lands at `target\release\wattage-monitor.exe`. It is self-contained — copy it anywhere.

```powershell
cargo test
```

## Requirements

Windows with a battery exposing the standard `GUID_DEVICE_BATTERY` interface. On a desktop with no battery the icon shows `N/A`.

## Why not adapter wattage?

Because Windows does not know it. **There is no universal way to read AC adapter output on Windows** — the ACPI specification has no such field, `Win32_PowerMeter` returns zero on most hardware, and vendor WMI classes carry static metadata rather than live power telemetry. The USB-C PD power contract is exposed only as an undocumented, Windows-proprietary blob.

So this tool reports battery flow, which is what the OS actually measures. To *estimate* adapter total, measure your discharge baseline on battery and add it to the charge rate on AC.

## Licence

MIT
