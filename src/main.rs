#![windows_subsystem = "windows"]

mod battery;
mod config;
mod icon;
mod tray;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Shell::NIN_SELECT;
use windows::Win32::UI::WindowsAndMessaging::*;

const WINDOW_CLASS: &str = "BatteryWattageWindow";
const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 1000;
const BATTERY_RETRY_INTERVAL_TICKS: u32 = 5;
/// Ticks without a usable reading before the label falls back to "N/A".
/// One tick is TIMER_INTERVAL_MS, so this is 10 seconds.
const STALE_LIMIT_TICKS: u32 = 10;
/// Readings smaller than this are treated as a charge threshold rather than
/// charge/discharge, so a near-zero rate does not flicker the label.
const THRESHOLD_WATTS: f64 = 1.0;
const NIN_KEYSELECT: u32 = NIN_SELECT + 1;
pub const WMAPP_TRAY: u32 = WM_USER + 1;

struct AppData {
    config: config::Config,
    battery: Option<battery::BatteryReader>,
    battery_retry_ticks: u32,
    ticks_since_reading: u32,
    had_reading: bool,
    showing_na: bool,
    current_icon: HICON,
    preset_names: Vec<&'static str>,
}

impl AppData {
    fn preset_names() -> Vec<&'static str> {
        config::color_presets()
            .into_iter()
            .map(|(n, _)| n)
            .collect()
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            let app_ptr = (*cs).lpCreateParams as *mut AppData;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_ptr as isize);
            SetTimer(hwnd, TIMER_ID, TIMER_INTERVAL_MS, None);
            LRESULT(0)
        }

        WM_TIMER => {
            if wparam.0 == TIMER_ID {
                let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppData;
                if !app_ptr.is_null() {
                    refresh_battery(hwnd, &mut *app_ptr);
                }
            }
            LRESULT(0)
        }

        WMAPP_TRAY => {
            let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppData;
            if !app_ptr.is_null() {
                let app = &mut *app_ptr;
                match (lparam.0 as u32) & 0xFFFF {
                    WM_CONTEXTMENU | WM_RBUTTONUP | WM_LBUTTONUP | NIN_SELECT | NIN_KEYSELECT => {
                        tray::show_menu(
                            hwnd,
                            &app.config,
                            &app.preset_names,
                            tray::startup_enabled(),
                        );
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }

        WM_COMMAND => {
            let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppData;
            if app_ptr.is_null() {
                return LRESULT(0);
            }
            let app = &mut *app_ptr;
            let cmd = (wparam.0 & 0xFFFF) as u16;

            match cmd {
                tray::CMD_BACKGROUND => {
                    app.config.background = !app.config.background;
                    config::save_config(&app.config);
                    refresh_battery(hwnd, app);
                }
                id if id >= tray::CMD_TEXTSCALE_BASE
                    && id < tray::CMD_TEXTSCALE_BASE + config::TEXT_SCALES.len() as u16 =>
                {
                    let idx = (id - tray::CMD_TEXTSCALE_BASE) as usize;
                    if let Some(&pct) = config::TEXT_SCALES.get(idx) {
                        app.config.text_scale = pct;
                        config::save_config(&app.config);
                        refresh_battery(hwnd, app);
                    }
                }
                tray::CMD_STARTUP => {
                    let enabled = tray::startup_enabled();
                    tray::set_startup(!enabled);
                }
                tray::CMD_ABOUT => {
                    show_about(hwnd);
                }
                tray::CMD_EXIT => {
                    let _ = DestroyWindow(hwnd);
                }
                id if id >= tray::CMD_COLOR_BASE
                    && id < tray::CMD_COLOR_BASE + app.preset_names.len() as u16 =>
                {
                    let idx = (id - tray::CMD_COLOR_BASE) as usize;
                    app.config.preset = app.preset_names[idx].to_string();
                    config::save_config(&app.config);
                    refresh_battery(hwnd, app);
                }
                _ => {}
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            let _ = KillTimer(hwnd, TIMER_ID);
            tray::remove(hwnd);

            let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppData;
            if !app_ptr.is_null() {
                let app = &mut *app_ptr;
                if !app.current_icon.is_invalid() {
                    let _ = DestroyIcon(app.current_icon);
                }
                let _ = Box::from_raw(app_ptr);
            }

            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => {
            let taskbar_created = {
                let wide: Vec<u16> = "TaskbarCreated\0".encode_utf16().collect();
                unsafe { RegisterWindowMessageW(PCWSTR::from_raw(wide.as_ptr())) }
            };
            if msg == taskbar_created {
                let app_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppData;
                if !app_ptr.is_null() {
                    let _ = tray::create(hwnd);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

fn refresh_battery(hwnd: HWND, app: &mut AppData) {
    if let Some(data) = read_battery(app).filter(has_usable_reading) {
        app.had_reading = true;
        app.ticks_since_reading = 0;
        app.showing_na = false;

        let preset = config::lookup_preset(&app.config.preset);
        let display = battery_display(&data);
        let color = if display.threshold {
            preset.idle
        } else if data.charging {
            preset.charging
        } else if data.discharging {
            preset.discharging
        } else {
            preset.idle
        };
        replace_icon(hwnd, app, &display.label, color);
        tray::update_tooltip(hwnd, &display.tooltip);
        return;
    }

    app.ticks_since_reading = app.ticks_since_reading.saturating_add(1);

    // Keep showing the last good value through brief read failures. Only fall
    // back to "N/A" if no reading has ever arrived (a machine with no battery)
    // or nothing has been read for STALE_LIMIT_TICKS.
    let stale = !app.had_reading || app.ticks_since_reading >= STALE_LIMIT_TICKS;
    if stale && (!app.showing_na || app.current_icon.is_invalid()) {
        app.showing_na = true;
        let display = no_battery_display();
        let color = config::lookup_preset(&app.config.preset).idle;
        replace_icon(hwnd, app, &display.label, color);
        tray::update_tooltip(hwnd, &display.tooltip);
    }
}

/// A read counts as usable if it carries a rate, or if it is an AC-online
/// threshold state where the driver legitimately reports no rate at all.
fn has_usable_reading(data: &battery::BatteryData) -> bool {
    data.rate.is_some() || (data.power_online && !data.charging && !data.discharging)
}

fn read_battery(app: &mut AppData) -> Option<battery::BatteryData> {
    if let Some(reader) = app.battery.as_ref() {
        if let Some(data) = reader.read() {
            return Some(data);
        }
        app.battery = None;
        app.battery_retry_ticks = BATTERY_RETRY_INTERVAL_TICKS.saturating_sub(1);
        return None;
    }

    if app.battery_retry_ticks > 0 {
        app.battery_retry_ticks -= 1;
        return None;
    }

    let reader = match battery::BatteryReader::open() {
        Ok(reader) => reader,
        Err(_) => {
            app.battery_retry_ticks = BATTERY_RETRY_INTERVAL_TICKS.saturating_sub(1);
            return None;
        }
    };

    match reader.read() {
        Some(data) => {
            app.battery = Some(reader);
            Some(data)
        }
        None => {
            app.battery_retry_ticks = BATTERY_RETRY_INTERVAL_TICKS.saturating_sub(1);
            None
        }
    }
}

struct DisplayText {
    label: String,
    tooltip: String,
    threshold: bool,
}

fn no_battery_display() -> DisplayText {
    DisplayText {
        label: "N/A".into(),
        tooltip: "No battery detected".into(),
        threshold: false,
    }
}

fn battery_display(data: &battery::BatteryData) -> DisplayText {
    let watts = data.rate.map(|rate| rate as f64 / 1000.0);
    // On AC, anything under +/-1 W is the charge threshold holding the battery
    // steady, not real charge/discharge worth showing as a number.
    let threshold = data.power_online
        && match watts {
            Some(value) => value.abs() < THRESHOLD_WATTS,
            None => !data.charging && !data.discharging,
        };
    let status = if data.charging {
        "Charging"
    } else if data.discharging {
        "Discharging"
    } else {
        "Idle"
    };
    let label = if threshold {
        "T".into()
    } else {
        watts
            .map(|value| format!("{}", value.round() as i32))
            .unwrap_or_else(|| "N/A".into())
    };

    let rate_line = if threshold {
        "Threshold (charge limit reached)".into()
    } else {
        match (watts, data.voltage) {
            (Some(value), Some(voltage)) if voltage > 0 => {
                let amps = value.abs() / (voltage as f64 / 1000.0);
                format!("{}  {:.0} W  ({:.2} A)", status, value, amps)
            }
            (Some(value), _) => format!("{}  {:.0} W", status, value),
            (None, _) => format!("{}  Rate unavailable", status),
        }
    };
    let voltage_line = data
        .voltage
        .map(|voltage| format!("Voltage: {:.2} V", voltage as f64 / 1000.0))
        .unwrap_or_else(|| "Voltage: unavailable".into());
    let battery_line = match (data.capacity, data.full_capacity) {
        (Some(capacity), Some(full_capacity)) if full_capacity > 0 => {
            let percentage = ((capacity as f64 / full_capacity as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u32;
            if data.capacity_relative {
                format!("Battery: {}%", percentage)
            } else {
                format!(
                    "Battery: {}%  ({} / {} mWh)",
                    percentage, capacity, full_capacity
                )
            }
        }
        _ => "Battery: unknown".into(),
    };

    DisplayText {
        label,
        tooltip: format!("{}\n{}\n{}", rate_line, voltage_line, battery_line),
        threshold,
    }
}

fn replace_icon(hwnd: HWND, app: &mut AppData, label: &str, color: [u8; 4]) {
    if let Ok(hicon) = icon::make_icon(
        label,
        color,
        app.config.background,
        app.config.text_scale as i32,
    ) {
        tray::update_icon(hwnd, hicon);
        if !app.current_icon.is_invalid() {
            let _ = unsafe { DestroyIcon(app.current_icon) };
        }
        app.current_icon = hicon;
    }
}

fn show_about(hwnd: HWND) {
    let text: Vec<u16> = "BatteryWattage\n\nShows real-time battery charge/discharge\nwattage in the system tray.\n\nWritten in Rust\nDirect IOCTL - no WMI, no PowerShell\n\ngithub.com/frankforges/BatteryWattage\0".encode_utf16().collect();
    let title: Vec<u16> = "About BatteryWattage\0".encode_utf16().collect();
    unsafe {
        MessageBoxW(
            hwnd,
            PCWSTR::from_raw(text.as_ptr()),
            PCWSTR::from_raw(title.as_ptr()),
            MESSAGEBOX_STYLE(MB_OK.0 | MB_ICONINFORMATION.0),
        );
    }
}

fn create_hidden_window(app_ptr: *mut AppData) -> Result<HWND> {
    let class_name: Vec<u16> = format!("{}\0", WINDOW_CLASS).encode_utf16().collect();

    let wc = WNDCLASSW {
        style: WNDCLASS_STYLES(0),
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: HINSTANCE::default(),
        hIcon: HICON::default(),
        hCursor: HCURSOR::default(),
        hbrBackground: HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
    };

    let atom = unsafe { RegisterClassW(&wc) };
    if atom == 0 {
        return Err(Error::new(E_FAIL, "RegisterClassW failed"));
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR::from_raw(class_name.as_ptr()),
            PCWSTR::from_raw(class_name.as_ptr()),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            None,
            Some(app_ptr as *const core::ffi::c_void),
        )
    }?;

    Ok(hwnd)
}

fn main() -> Result<()> {
    let cfg = config::load_config();
    config::save_config(&cfg); // ensure registry key and values exist on first launch
    let preset_names = AppData::preset_names();

    let app_data = Box::new(AppData {
        config: cfg,
        battery: None,
        battery_retry_ticks: 0,
        ticks_since_reading: 0,
        had_reading: false,
        showing_na: false,
        current_icon: HICON::default(),
        preset_names,
    });

    let app_ptr = Box::into_raw(app_data);
    let hwnd = create_hidden_window(app_ptr)?;

    tray::create(hwnd)?;

    // Trigger first battery read immediately
    unsafe {
        let _ = PostMessageW(hwnd, WM_TIMER, WPARAM(TIMER_ID), LPARAM(0));
    };

    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if ret.0 == 0 || ret.0 == -1 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_battery_uses_na_icon_and_clear_tooltip() {
        let display = no_battery_display();

        assert_eq!(display.label, "N/A");
        assert_eq!(display.tooltip, "No battery detected");
    }

    #[test]
    fn unknown_measurements_are_not_rendered_as_sentinel_values() {
        let display = battery_display(&battery::BatteryData::default());

        assert_eq!(display.label, "N/A");
        assert!(display.tooltip.contains("Rate unavailable"));
        assert!(display.tooltip.contains("Voltage: unavailable"));
        assert!(display.tooltip.contains("Battery: unknown"));
        assert!(!display.tooltip.contains("4294967295"));
        assert!(!display.tooltip.contains("-2147484"));
    }

    #[test]
    fn conservation_mode_uses_threshold_icon_and_tooltip() {
        let data = battery::BatteryData {
            rate: Some(0),
            voltage: Some(12_000),
            capacity: Some(40_000),
            full_capacity: Some(50_000),
            power_online: true,
            ..Default::default()
        };

        let display = battery_display(&data);

        assert_eq!(display.label, "T");
        assert!(display
            .tooltip
            .starts_with("Threshold (charge limit reached)"));
    }

    #[test]
    fn offline_idle_battery_remains_idle() {
        let data = battery::BatteryData {
            rate: Some(0),
            voltage: Some(12_000),
            capacity: Some(40_000),
            full_capacity: Some(50_000),
            ..Default::default()
        };

        let display = battery_display(&data);

        assert_eq!(display.label, "0");
        assert!(display.tooltip.starts_with("Idle"));
    }

    #[test]
    fn sub_one_watt_on_ac_is_treated_as_threshold() {
        for milliwatts in [900, -900, 800, -800, 999, -999, 0] {
            let data = battery::BatteryData {
                rate: Some(milliwatts),
                voltage: Some(12_000),
                power_online: true,
                charging: milliwatts > 0,
                discharging: milliwatts < 0,
                ..Default::default()
            };

            let display = battery_display(&data);

            assert_eq!(display.label, "T", "{} mW should read as threshold", milliwatts);
            assert!(display.threshold);
        }
    }

    #[test]
    fn one_watt_or_more_on_ac_shows_the_number() {
        let data = battery::BatteryData {
            rate: Some(1_400),
            voltage: Some(12_000),
            power_online: true,
            charging: true,
            ..Default::default()
        };

        let display = battery_display(&data);

        assert_eq!(display.label, "1");
        assert!(!display.threshold);
        assert!(display.tooltip.starts_with("Charging"));
    }

    #[test]
    fn sub_one_watt_on_battery_is_not_a_threshold() {
        // Not AC-online, so a near-zero rate is idle, not a charge limit.
        let data = battery::BatteryData {
            rate: Some(-900),
            voltage: Some(12_000),
            discharging: true,
            ..Default::default()
        };

        let display = battery_display(&data);

        assert_eq!(display.label, "-1");
        assert!(!display.threshold);
    }

    #[test]
    fn readings_without_a_rate_are_not_usable() {
        // A successful read with BATTERY_UNKNOWN_RATE must not count as a
        // reading, otherwise it repaints the tray as "N/A" mid-charge.
        let unknown_rate = battery::BatteryData {
            charging: true,
            power_online: true,
            ..Default::default()
        };
        assert!(!has_usable_reading(&unknown_rate));

        let with_rate = battery::BatteryData {
            rate: Some(12_000),
            charging: true,
            power_online: true,
            ..Default::default()
        };
        assert!(has_usable_reading(&with_rate));
    }

    #[test]
    fn threshold_without_a_rate_still_counts_as_a_reading() {
        let data = battery::BatteryData {
            power_online: true,
            ..Default::default()
        };

        assert!(has_usable_reading(&data));
        assert_eq!(battery_display(&data).label, "T");
    }

    #[test]
    fn relative_capacity_is_not_labeled_as_watts_or_mwh() {
        let data = battery::BatteryData {
            capacity: Some(50),
            full_capacity: Some(100),
            capacity_relative: true,
            charging: true,
            ..Default::default()
        };

        let display = battery_display(&data);

        assert_eq!(display.label, "N/A");
        assert!(display.tooltip.contains("Battery: 50%"));
        assert!(!display.tooltip.contains("mWh"));
        assert!(!display.tooltip.contains(" W"));
    }
}
