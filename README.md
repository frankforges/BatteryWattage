<div align="center">

# ⚡ BatteryWattage

**See exactly how fast your laptop battery is charging — or quietly draining while plugged in.**

A live wattage readout that sits in your Windows system tray as a number, not an icon.

[![Download](https://img.shields.io/badge/download-latest%20release-2ea043?style=for-the-badge)](https://github.com/frankforges/BatteryWattage/releases/latest)
[![Platform](https://img.shields.io/badge/windows-10%20%7C%2011-0078d4?style=flat-square)](#requirements)
[![Size](https://img.shields.io/badge/size-229%20KB-blue?style=flat-square)](#why-its-small)
[![No installer](https://img.shields.io/badge/install-none%20needed-success?style=flat-square)](#installation)
[![Licence](https://img.shields.io/badge/licence-MIT-lightgrey?style=flat-square)](LICENSE)

</div>

---

## The problem it solves

Windows tells you a percentage and a vague "2 hours remaining." It never tells you the number that actually matters: **how many watts are moving, right now, in which direction.**

That gap hides real problems. Here is the one that prompted this tool:

> A laptop plugged into its own charger, under heavy load, was **discharging**. The adapter could not keep up with what the CPU and GPU were pulling, so the battery quietly drained at wall power. Nothing in Windows surfaces this. The battery icon shows "plugged in" and says nothing more.

That is worth knowing about your hardware. It means the machine is not suited to being a permanent workstation under load, and you would never discover it from the built-in indicators.

Nothing available did this in a practical, always-visible way — so this exists.

## What it looks like

The design follows [Core Temp](https://www.alcpu.com/CoreTemp/)'s approach: the tray *is* the readout. No window, no dashboard, no background service. Just a live number that sits comfortably next to your Core Temp CPU icons.

| Tray shows | Meaning |
|:---:|---|
| **`18`** | Charging at 18 W |
| **`-12`** | Discharging at 12 W — *including while plugged in* |
| **`T`** | **Threshold.** AC connected, battery deliberately held (charge limit / conservation mode). Also any reading under ±1 W on AC. |
| **`N/A`** | No battery present, or no reading for 10 seconds |

Colour tracks state: **green** charging, **amber** discharging, **blue-grey** idle. Hover for a tooltip with watts, amps, terminal voltage, and charge percentage.

## Installation

No installer. No dependencies. No admin rights.

1. **[Download `BatteryWattage.exe`](https://github.com/frankforges/BatteryWattage/releases/latest)**
2. Put it anywhere you like
3. Double-click it

That is the whole process. To have it start with Windows, right-click the tray icon → **Run at startup**.

To uninstall: right-click → **Exit**, turn off *Run at startup* first if you enabled it, and delete the file.

## Configuration

**There is no config file.** Settings live in the Windows registry, which is why the executable stays a single portable file you can drop on a USB stick.

Everything is set from the right-click menu:

- **Colour presets** — Default, All White, All Green, High Contrast
- **Background pill** — the rounded backdrop behind the number, on or off
- **Text scale** — 75% / 100% / 125% / 150% / 200%, for high-DPI displays
- **Run at startup** — toggles a registry Run entry
- **About** / **Exit**

For the curious, settings persist at `HKEY_CURRENT_USER\Software\BatteryWattage`:

| Value | Type | Meaning |
|---|---|---|
| `Preset` | string | Colour preset name |
| `Background` | dword | `1` background pill on, `0` off |
| `TextScale` | dword | Font scale percentage |

"Run at startup" writes `BatteryWattage` to `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`.

Settings follow your Windows user account, not the executable — move the `.exe` and your preferences come with you.

## Why it's small

**229 KB, no runtime, no installer.** It talks to the battery driver directly through `DeviceIoControl`, reading the same ACPI data Windows itself uses.

The predecessor to this tool was a Python script that shelled out to PowerShell to query WMI once per second. That worked, but it was a 15.7 MB bundle spawning a process every tick, all day. The Rust rewrite is **69× smaller**, spawns nothing, and idles at a few MB of RAM.

The MSVC runtime is statically linked, so there is genuinely nothing to install alongside it.

## Why battery flow, and not adapter wattage?

Because Windows does not know your adapter's output. This is a real gap, not an oversight in this tool:

- The ACPI specification has **no field** for AC adapter wattage
- `Win32_PowerMeter` returns zero on most consumer hardware
- Vendor WMI classes carry static metadata, not live power telemetry
- The USB-C Power Delivery contract is exposed only as an undocumented, Windows-proprietary binary blob

What the OS *does* measure is net flow into or out of the battery cells:

```
adapter output = system consumption + battery charge rate
```

So a reading of `-12` while plugged in tells you something precise and useful: your system is drawing 12 W more than the adapter is supplying.

### Why `T` instead of `0`

When a charge threshold holds your battery at, say, 80%, net flow sits near zero — while the adapter is still delivering real power to run the machine. Showing `0` would imply nothing is happening, which is wrong. `T` says *deliberately held*.

Readings under ±1 W on AC fold into `T` for the same reason: a battery hovering at 0.4 W is being held, not charged, and flickering between `0` and `1` is noise, not information.

## Requirements

Windows 10 or 11 with a battery exposing the standard `GUID_DEVICE_BATTERY` interface — that is every normal laptop. On a desktop with no battery, the icon shows `N/A`.

## Build from source

```powershell
cargo build --release
cargo test
```

The binary lands at `target\release\BatteryWattage.exe`. `.cargo/config.toml` pins static CRT linking so your build is as portable as the released one.

## Licence

MIT — see [LICENSE](LICENSE).
