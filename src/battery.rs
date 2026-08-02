use windows::core::*;
use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::Power::*;
use windows::Win32::System::IO::*;

#[derive(Debug, Clone, Default)]
pub struct BatteryData {
    pub rate: Option<i32>,
    pub voltage: Option<u32>,
    pub capacity: Option<u32>,
    pub full_capacity: Option<u32>,
    pub capacity_relative: bool,
    #[allow(dead_code)]
    pub power_state: u32,
    pub charging: bool,
    pub discharging: bool,
    pub power_online: bool,
}

pub struct BatteryReader {
    handle: HANDLE,
    tag: u32,
}

fn open_battery_device() -> Result<HANDLE> {
    let flags = SETUP_DI_GET_CLASS_DEVS_FLAGS(DIGCF_PRESENT.0 | DIGCF_DEVICEINTERFACE.0);
    let hdev = unsafe { SetupDiGetClassDevsW(Some(&GUID_DEVICE_BATTERY), None, None, flags) }?;

    let mut did = SP_DEVICE_INTERFACE_DATA {
        cbSize: core::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
        ..Default::default()
    };

    if unsafe { SetupDiEnumDeviceInterfaces(hdev, None, &GUID_DEVICE_BATTERY, 0, &mut did) }
        .is_err()
    {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(hdev);
        };
        return Err(Error::new(E_FAIL, "No battery interface found"));
    }

    let mut required = 0u32;
    unsafe {
        let _ = SetupDiGetDeviceInterfaceDetailW(hdev, &did, None, 0, Some(&mut required), None);
    };
    if required == 0 {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(hdev);
        };
        return Err(Error::new(E_FAIL, "Zero-size battery device path"));
    }

    let mut buf: Vec<u8> = vec![0u8; required as usize];
    let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
    unsafe {
        (*detail).cbSize = core::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
    }

    if unsafe { SetupDiGetDeviceInterfaceDetailW(hdev, &did, Some(detail), required, None, None) }
        .is_err()
    {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(hdev);
        };
        return Err(Error::new(E_FAIL, "Failed to get battery device path"));
    }

    let device_path = unsafe {
        let path_start = (*detail).DevicePath.as_ptr();
        let len = (0usize..).find(|&i| *path_start.add(i) == 0).unwrap_or(0);
        PCWSTR::from_raw(std::slice::from_raw_parts(path_start, len).as_ptr())
    };

    let handle = unsafe {
        CreateFileW(
            device_path,
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };

    unsafe {
        let _ = SetupDiDestroyDeviceInfoList(hdev);
    };
    handle
}

fn get_battery_tag(handle: HANDLE) -> Result<u32> {
    let mut tag: u32 = 0;
    let mut returned: u32 = 0;
    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_BATTERY_QUERY_TAG,
            None,
            0,
            Some(&mut tag as *mut u32 as *mut core::ffi::c_void),
            core::mem::size_of::<u32>() as u32,
            Some(&mut returned),
            None,
        )
    }?;
    if tag == BATTERY_TAG_INVALID {
        Err(Error::new(E_FAIL, "Battery tag is invalid"))
    } else {
        Ok(tag)
    }
}

impl BatteryReader {
    pub fn open() -> Result<Self> {
        let handle = open_battery_device()?;
        match get_battery_tag(handle) {
            Ok(tag) => Ok(Self { handle, tag }),
            Err(error) => {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                Err(error)
            }
        }
    }

    pub fn read(&self) -> Option<BatteryData> {
        let wait_status = BATTERY_WAIT_STATUS {
            BatteryTag: self.tag,
            Timeout: 0,
            PowerState: 0,
            LowCapacity: 0,
            HighCapacity: 0,
        };

        let mut status = BATTERY_STATUS::default();
        let mut returned: u32 = 0;

        if unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_BATTERY_QUERY_STATUS,
                Some(&wait_status as *const BATTERY_WAIT_STATUS as *const core::ffi::c_void),
                core::mem::size_of::<BATTERY_WAIT_STATUS>() as u32,
                Some(&mut status as *mut BATTERY_STATUS as *mut core::ffi::c_void),
                core::mem::size_of::<BATTERY_STATUS>() as u32,
                Some(&mut returned),
                None,
            )
        }
        .is_err()
        {
            return None;
        }

        let power_state = status.PowerState;
        let charging = (power_state & BATTERY_CHARGING) != 0;
        let discharging = (power_state & BATTERY_DISCHARGING) != 0;
        let power_online = (power_state & BATTERY_POWER_ON_LINE) != 0;

        let query_info = BATTERY_QUERY_INFORMATION {
            BatteryTag: self.tag,
            InformationLevel: BatteryInformation,
            AtRate: 0,
        };
        let mut bat_info = BATTERY_INFORMATION::default();
        let mut returned2: u32 = 0;

        let (full_capacity, capacity_relative) = unsafe {
            match DeviceIoControl(
                self.handle,
                IOCTL_BATTERY_QUERY_INFORMATION,
                Some(&query_info as *const BATTERY_QUERY_INFORMATION as *const core::ffi::c_void),
                core::mem::size_of::<BATTERY_QUERY_INFORMATION>() as u32,
                Some(&mut bat_info as *mut BATTERY_INFORMATION as *mut core::ffi::c_void),
                core::mem::size_of::<BATTERY_INFORMATION>() as u32,
                Some(&mut returned2),
                None,
            ) {
                Ok(()) => (
                    (bat_info.FullChargedCapacity != BATTERY_UNKNOWN_CAPACITY)
                        .then_some(bat_info.FullChargedCapacity),
                    (bat_info.Capabilities & BATTERY_CAPACITY_RELATIVE) != 0,
                ),
                Err(_) => (None, false),
            }
        };

        Some(BatteryData {
            rate: (status.Rate as u32 != BATTERY_UNKNOWN_RATE && !capacity_relative)
                .then_some(status.Rate),
            voltage: (status.Voltage != BATTERY_UNKNOWN_VOLTAGE).then_some(status.Voltage),
            capacity: (status.Capacity != BATTERY_UNKNOWN_CAPACITY).then_some(status.Capacity),
            full_capacity,
            capacity_relative,
            power_state,
            charging,
            discharging,
            power_online,
        })
    }
}

impl Drop for BatteryReader {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
