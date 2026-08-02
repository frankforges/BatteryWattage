use windows::core::*;
use windows::Win32::System::Registry::*;

/// Registry path for all config
const REG_KEY: &str = "Software\\WattageMonitor\0";

/// Text scale options (font size multiplier as percentage)
pub const TEXT_SCALES: &[u32] = &[75, 100, 125, 150, 200];
pub const DEFAULT_TEXT_SCALE: u32 = 125;

#[derive(Debug, Clone)]
pub struct ColorPreset {
    pub charging: [u8; 4],
    pub discharging: [u8; 4],
    pub idle: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct Config {
    pub preset: String,
    pub background: bool,
    pub text_scale: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            preset: "Default".into(),
            background: true,
            text_scale: DEFAULT_TEXT_SCALE,
        }
    }
}

pub fn color_presets() -> Vec<(&'static str, ColorPreset)> {
    vec![
        (
            "Default",
            ColorPreset {
                charging: [30, 210, 60, 255],
                discharging: [255, 140, 20, 255],
                idle: [120, 180, 220, 255],
            },
        ),
        (
            "All White",
            ColorPreset {
                charging: [220, 220, 220, 255],
                discharging: [200, 200, 200, 255],
                idle: [180, 180, 180, 255],
            },
        ),
        (
            "All Green",
            ColorPreset {
                charging: [30, 210, 60, 255],
                discharging: [100, 180, 60, 255],
                idle: [60, 150, 80, 255],
            },
        ),
        (
            "High Contrast",
            ColorPreset {
                charging: [0, 255, 0, 255],
                discharging: [255, 60, 60, 255],
                idle: [100, 180, 255, 255],
            },
        ),
    ]
}

pub fn lookup_preset(name: &str) -> ColorPreset {
    color_presets()
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, p)| p.clone())
        .unwrap_or_else(|| {
            color_presets()
                .iter()
                .find(|(n, _)| *n == "Default")
                .map(|(_, p)| p.clone())
                .unwrap()
        })
}

// ── Registry helpers ──────────────────────────────────────────────

fn open_config_key(access: REG_SAM_FLAGS) -> Option<HKEY> {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let subkey: Vec<u16> = REG_KEY.encode_utf16().collect();
        let r = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(subkey.as_ptr()),
            0,
            None,
            REG_OPEN_CREATE_OPTIONS(0),
            access,
            None,
            &mut hkey,
            None,
        );
        if r.0 == 0 {
            Some(hkey)
        } else {
            None
        }
    }
}

fn read_string(hkey: HKEY, name: &str, default: &str) -> String {
    unsafe {
        let name_w: Vec<u16> = format!("{}\0", name).encode_utf16().collect();
        let mut data_type = REG_VALUE_TYPE(0);
        let mut data_size: u32 = 512;
        let mut buf: Vec<u8> = vec![0u8; 512];
        let r = RegQueryValueExW(
            hkey,
            PCWSTR::from_raw(name_w.as_ptr()),
            None,
            Some(&mut data_type),
            Some(buf.as_mut_ptr()),
            Some(&mut data_size),
        );
        if r.0 == 0 && data_type == REG_SZ && data_size >= 2 {
            // Decode UTF-16LE from bytes
            let u16_count = (data_size as usize) / 2;
            let u16_slice: &[u16] =
                std::slice::from_raw_parts(buf.as_ptr() as *const u16, u16_count);
            // Strip null terminator
            let end = u16_slice.iter().position(|&c| c == 0).unwrap_or(u16_count);
            String::from_utf16_lossy(&u16_slice[..end])
        } else {
            default.to_string()
        }
    }
}

fn write_string(hkey: HKEY, name: &str, value: &str) {
    unsafe {
        let name_w: Vec<u16> = format!("{}\0", name).encode_utf16().collect();
        let mut val_w: Vec<u16> = value.encode_utf16().collect();
        val_w.push(0);
        let bytes = std::slice::from_raw_parts(val_w.as_ptr() as *const u8, val_w.len() * 2);
        let _ = RegSetValueExW(
            hkey,
            PCWSTR::from_raw(name_w.as_ptr()),
            0,
            REG_SZ,
            Some(bytes),
        );
    }
}

fn read_dword(hkey: HKEY, name: &str, default: u32) -> u32 {
    unsafe {
        let name_w: Vec<u16> = format!("{}\0", name).encode_utf16().collect();
        let mut data_type = REG_VALUE_TYPE(0);
        let mut value: u32 = 0;
        let mut data_size: u32 = 4;
        let r = RegQueryValueExW(
            hkey,
            PCWSTR::from_raw(name_w.as_ptr()),
            None,
            Some(&mut data_type),
            Some(&mut value as *mut u32 as *mut u8),
            Some(&mut data_size),
        );
        if r.0 == 0 && data_type == REG_DWORD {
            value
        } else {
            default
        }
    }
}

fn write_dword(hkey: HKEY, name: &str, value: u32) {
    unsafe {
        let name_w: Vec<u16> = format!("{}\0", name).encode_utf16().collect();
        let bytes: [u8; 4] = value.to_le_bytes();
        let _ = RegSetValueExW(
            hkey,
            PCWSTR::from_raw(name_w.as_ptr()),
            0,
            REG_DWORD,
            Some(&bytes),
        );
    }
}

// ── Public config API ─────────────────────────────────────────────

pub fn load_config() -> Config {
    let Some(hkey) = open_config_key(REG_SAM_FLAGS(KEY_READ.0 | KEY_WRITE.0)) else {
        return Config::default();
    };

    // Delete orphaned values from previous versions
    let orphan: Vec<u16> = "IconSize\0".encode_utf16().collect();
    unsafe {
        let _ = RegDeleteValueW(hkey, PCWSTR::from_raw(orphan.as_ptr()));
    }

    let mut cfg = Config::default();
    cfg.preset = read_string(hkey, "Preset", &cfg.preset);
    cfg.background = read_dword(hkey, "Background", if cfg.background { 1 } else { 0 }) != 0;
    cfg.text_scale = read_dword(hkey, "TextScale", cfg.text_scale);

    // Validate
    let valid_preset = color_presets().iter().any(|(n, _)| n == &cfg.preset);
    if !valid_preset {
        cfg.preset = "Default".into();
    }
    if !TEXT_SCALES.contains(&cfg.text_scale) {
        cfg.text_scale = DEFAULT_TEXT_SCALE;
    }

    let _ = unsafe { RegCloseKey(hkey) };
    cfg
}

pub fn save_config(cfg: &Config) {
    let Some(hkey) = open_config_key(KEY_WRITE) else {
        return;
    };

    write_string(hkey, "Preset", &cfg.preset);
    write_dword(hkey, "Background", if cfg.background { 1 } else { 0 });
    write_dword(hkey, "TextScale", cfg.text_scale);

    let _ = unsafe { RegCloseKey(hkey) };
}
