use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config::{self, Config};

pub const CMD_COLOR_BASE: u16 = 1000;
pub const CMD_BACKGROUND: u16 = 1001;
pub const CMD_STARTUP: u16 = 1002;
pub const CMD_ABOUT: u16 = 1003;
pub const CMD_EXIT: u16 = 1004;
pub const CMD_TEXTSCALE_BASE: u16 = 2000;

const UID: u32 = 1;

/// Create the tray icon. Returns Ok(()) if the icon was added successfully.
pub fn create(hwnd: HWND) -> Result<()> {
    let mut tip = [0u16; 128];
    let src: Vec<u16> = "Wattage Monitor\0".encode_utf16().collect();
    let len = src.len().min(128);
    tip[..len].copy_from_slice(&src[..len]);

    let nid = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: UID,
        uFlags: NOTIFY_ICON_DATA_FLAGS(NIF_MESSAGE.0 | NIF_ICON.0 | NIF_TIP.0),
        uCallbackMessage: crate::WMAPP_TRAY,
        hIcon: HICON::default(),
        szTip: tip,
        dwState: NOTIFY_ICON_STATE::default(),
        dwStateMask: NOTIFY_ICON_STATE::default(),
        szInfo: [0u16; 256],
        Anonymous: NOTIFYICONDATAW_0 { uVersion: 0 },
        szInfoTitle: [0u16; 64],
        dwInfoFlags: NOTIFY_ICON_INFOTIP_FLAGS::default(),
        guidItem: GUID::default(),
        hBalloonIcon: HICON::default(),
    };

    let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
    if ok.0 == 0 {
        return Err(Error::new(E_FAIL, "NIM_ADD failed"));
    }

    // Set version 4
    let nid_v = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        uID: UID,
        Anonymous: NOTIFYICONDATAW_0 {
            uVersion: NOTIFYICON_VERSION_4,
        },
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid_v);
    }

    Ok(())
}

/// Update the tray icon.
pub fn update_icon(hwnd: HWND, icon: HICON) {
    let nid = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: UID,
        uFlags: NIF_ICON,
        hIcon: icon,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// Update the tooltip text.
pub fn update_tooltip(hwnd: HWND, tip: &str) {
    let mut sz = [0u16; 128];
    let wide: Vec<u16> = format!("{}\0", tip).encode_utf16().collect();
    let len = wide.len().min(128);
    sz[..len].copy_from_slice(&wide[..len]);

    let nid = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: UID,
        uFlags: NIF_TIP,
        szTip: sz,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// Remove the tray icon.
pub fn remove(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: UID,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

/// Show the right-click context menu.
pub fn show_menu(hwnd: HWND, config: &Config, preset_names: &[&str], startup_enabled: bool) {
    unsafe {
        let menu = match CreatePopupMenu() {
            Ok(m) => m,
            Err(_) => return,
        };

        // Color submenu
        let color_menu = match CreatePopupMenu() {
            Ok(m) => m,
            Err(_) => {
                let _ = DestroyMenu(menu);
                return;
            }
        };

        for (i, name) in preset_names.iter().enumerate() {
            let flags = if *name == config.preset {
                MENU_ITEM_FLAGS(MF_STRING.0 | MF_CHECKED.0)
            } else {
                MF_STRING
            };
            let wide: Vec<u16> = format!("{}\0", name).encode_utf16().collect();
            let _ = AppendMenuW(
                color_menu,
                flags,
                (CMD_COLOR_BASE + i as u16) as usize,
                PCWSTR::from_raw(wide.as_ptr()),
            );
        }

        let color_wide: Vec<u16> = "Color\0".encode_utf16().collect();
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(MF_STRING.0 | MF_POPUP.0),
            color_menu.0 as usize,
            PCWSTR::from_raw(color_wide.as_ptr()),
        );

        // Text scale submenu
        let scale_menu = match CreatePopupMenu() {
            Ok(m) => m,
            Err(_) => {
                let _ = DestroyMenu(menu);
                return;
            }
        };
        for (i, &pct) in config::TEXT_SCALES.iter().enumerate() {
            let flags = if pct == config.text_scale {
                MENU_ITEM_FLAGS(MF_STRING.0 | MF_CHECKED.0)
            } else {
                MF_STRING
            };
            let label: Vec<u16> = format!("{}%\0", pct).encode_utf16().collect();
            let _ = AppendMenuW(
                scale_menu,
                flags,
                (CMD_TEXTSCALE_BASE + i as u16) as usize,
                PCWSTR::from_raw(label.as_ptr()),
            );
        }
        let scale_wide: Vec<u16> = "Text scale\0".encode_utf16().collect();
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(MF_STRING.0 | MF_POPUP.0),
            scale_menu.0 as usize,
            PCWSTR::from_raw(scale_wide.as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        // Background pill toggle
        let bg_flags = if config.background {
            MENU_ITEM_FLAGS(MF_STRING.0 | MF_CHECKED.0)
        } else {
            MF_STRING
        };
        let bg_wide: Vec<u16> = "Background pill\0".encode_utf16().collect();
        let _ = AppendMenuW(
            menu,
            bg_flags,
            CMD_BACKGROUND as usize,
            PCWSTR::from_raw(bg_wide.as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        // Auto-start toggle
        let start_flags = if startup_enabled {
            MENU_ITEM_FLAGS(MF_STRING.0 | MF_CHECKED.0)
        } else {
            MF_STRING
        };
        let start_wide: Vec<u16> = "Run at startup\0".encode_utf16().collect();
        let _ = AppendMenuW(
            menu,
            start_flags,
            CMD_STARTUP as usize,
            PCWSTR::from_raw(start_wide.as_ptr()),
        );

        // About
        let about_wide: Vec<u16> = "About\0".encode_utf16().collect();
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            CMD_ABOUT as usize,
            PCWSTR::from_raw(about_wide.as_ptr()),
        );

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        // Exit
        let exit_wide: Vec<u16> = "E&xit\0".encode_utf16().collect();
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            CMD_EXIT as usize,
            PCWSTR::from_raw(exit_wide.as_ptr()),
        );

        // Show
        let _ = SetForegroundWindow(hwnd);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);

        let tp_flags = TRACK_POPUP_MENU_FLAGS(TPM_RIGHTBUTTON.0 | TPM_BOTTOMALIGN.0);
        let _ = TrackPopupMenu(menu, tp_flags, cursor.x, cursor.y, 0, hwnd, None);

        let _ = PostMessageW(hwnd, WM_NULL, None, None);
        let _ = DestroyMenu(menu);
    }
}

/// Check if auto-start is enabled via registry Run key.
pub fn startup_enabled() -> bool {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let subkey: Vec<u16> = RUN_KEY.encode_utf16().collect();
        let r = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        );
        if r.0 != 0 {
            return false;
        }
        let name: Vec<u16> = VALUE_NAME.encode_utf16().collect();
        let mut data_type = REG_VALUE_TYPE(0);
        let mut data_size: u32 = 0;
        let result = RegQueryValueExW(
            hkey,
            PCWSTR::from_raw(name.as_ptr()),
            None,
            Some(&mut data_type),
            None,
            Some(&mut data_size),
        );
        let _ = RegCloseKey(hkey);
        result.0 == 0 // ERROR_SUCCESS means value exists
    }
}

/// Enable/disable auto-start via HKCU\Software\Microsoft\Windows\CurrentVersion\Run.
/// Pure Win32 registry — no COM, no PowerShell, no filesystem dependencies.
pub fn set_startup(enable: bool) {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let subkey: Vec<u16> = RUN_KEY.encode_utf16().collect();
        let r = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(subkey.as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        );
        if r.0 != 0 {
            return;
        }

        if enable {
            let mut buf = [0u16; 512];
            let len = GetModuleFileNameW(None, &mut buf);
            if len > 0 && (len as usize) < 512 {
                let path = &buf[..len as usize];
                // REG_SZ: null-terminated UTF-16LE as bytes
                let mut wide: Vec<u16> = path.to_vec();
                wide.push(0);
                let bytes = core::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);

                let name: Vec<u16> = VALUE_NAME.encode_utf16().collect();
                let _ = RegSetValueExW(
                    hkey,
                    PCWSTR::from_raw(name.as_ptr()),
                    0,
                    REG_SZ,
                    Some(bytes),
                );
            }
        } else {
            let name: Vec<u16> = VALUE_NAME.encode_utf16().collect();
            let _ = RegDeleteValueW(hkey, PCWSTR::from_raw(name.as_ptr()));
        }

        let _ = RegCloseKey(hkey);
    }
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0";
const VALUE_NAME: &str = "WattageMonitor\0";
