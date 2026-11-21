//! The system commands (R-020). Each one calls the Windows API directly: no `shutdown.exe`, so no
//! command line is ever built from a string. The tests never run the destructive ones.
use std::ffi::c_void;
use std::io;
use std::net::Ipv4Addr;

use windows::core::{w, Error, Result, HSTRING};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_NOT_ALL_ASSIGNED, ERROR_NO_DATA,
    E_UNEXPECTED, HANDLE, LPARAM, LUID, NO_ERROR, WPARAM,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::NetworkManagement::IpHelper::{
    GetIfEntry, GetIpAddrTable, IF_OPER_STATUS_OPERATIONAL, IF_TYPE_SOFTWARE_LOOPBACK, MIB_IFROW,
    MIB_IPADDRROW_XP, MIB_IPADDRTABLE, MIB_IPADDR_DELETED, MIB_IPADDR_DISCONNECTED,
    MIB_IPADDR_PRIMARY,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD,
};
use windows::Win32::System::Shutdown::{
    ExitWindowsEx, LockWorkStation, EWX_LOGOFF, EWX_REBOOT, EWX_SHUTDOWN, EXIT_WINDOWS_FLAGS,
    SHUTDOWN_REASON,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, SendNotifyMessageW, HWND_BROADCAST, SC_MONITORPOWER, SMTO_ABORTIFHUNG,
    WM_SETTINGCHANGE, WM_SYSCOMMAND,
};

/// Where Windows keeps the light or dark choice.
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const APPS_VALUE: &str = "AppsUseLightTheme";
const SYSTEM_VALUE: &str = "SystemUsesLightTheme";

pub fn lock() -> io::Result<()> {
    // SAFETY: no arguments; locks the interactive session.
    Ok(unsafe { LockWorkStation() }?)
}

/// Sleep (S3 on this PC). `hibernate` writes the memory to disk and powers off instead.
pub fn suspend(hibernate: bool) -> io::Result<()> {
    // SAFETY: no pointers; not forced, wake events stay enabled.
    if unsafe { SetSuspendState(hibernate, false, false) } {
        Ok(())
    } else {
        Err(Error::from_thread().into())
    }
}

/// Powers off displays using `WM_SYSCOMMAND` / `SC_MONITORPOWER` without blocking on hung windows.
pub fn sleep_displays() -> io::Result<()> {
    // SAFETY: broadcasts asynchronously with `SendNotifyMessageW` so hung top-level windows cannot
    // block the caller; `lparam = 2` is documented display-off.
    Ok(unsafe {
        SendNotifyMessageW(
            HWND_BROADCAST,
            WM_SYSCOMMAND,
            WPARAM(SC_MONITORPOWER as usize),
            LPARAM(2),
        )
    }?)
}

struct ComGuard(bool);

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: pairs a successful `CoInitializeEx` on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

fn with_audio_endpoint<T>(f: impl FnOnce(&IAudioEndpointVolume) -> Result<T>) -> io::Result<T> {
    // SAFETY: initializes COM on this thread if not yet initialized; `ComGuard` drops after
    // `endpoint` is released.
    let _com = ComGuard(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() });
    // SAFETY: documented Core Audio COM sequence (`IMMDeviceEnumerator` -> `IAudioEndpointVolume`).
    let endpoint: IAudioEndpointVolume = unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        device.Activate(CLSCTX_ALL, None)?
    };
    let out = f(&endpoint)?;
    drop(endpoint);
    Ok(out)
}

/// Reads `(muted, scalar_volume)` from the default multimedia render endpoint without mutating it.
pub fn audio_endpoint_state() -> io::Result<(bool, f32)> {
    with_audio_endpoint(|ep| {
        // SAFETY: `ep` is an active `IAudioEndpointVolume` queried read-only.
        unsafe { Ok((ep.GetMute()?.as_bool(), ep.GetMasterVolumeLevelScalar()?)) }
    })
}

/// Toggles the mute state of the default multimedia render endpoint.
pub fn toggle_mute() -> io::Result<()> {
    with_audio_endpoint(|ep| {
        // SAFETY: `ep` is an active `IAudioEndpointVolume`; `GetMute`/`SetMute` take no event GUID.
        unsafe {
            let muted = ep.GetMute()?.as_bool();
            ep.SetMute(!muted, std::ptr::null())
        }
    })
}

/// Sets the master scalar volume of the default multimedia render endpoint (`0..=100`).
pub fn set_volume(percent: u8) -> io::Result<()> {
    let level = f32::from(percent.min(100)) / 100.0;
    with_audio_endpoint(|ep| {
        // SAFETY: `ep` is an active `IAudioEndpointVolume`; `level` is in `0.0..=1.0`.
        unsafe { ep.SetMasterVolumeLevelScalar(level, std::ptr::null()) }
    })
}

/// A candidate IPv4 interface address collected from the Windows IP Helper tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAddr {
    pub ip: Ipv4Addr,
    pub operational: bool,
    pub loopback: bool,
    pub primary: bool,
    pub disconnected_or_deleted: bool,
}

/// Picks the best active non-loopback local IPv4 address (preferring non-link-local and primary).
pub fn select_local_ip(addrs: &[LocalAddr]) -> Option<Ipv4Addr> {
    let pick = |allow_link_local: bool| {
        let mut eligible = addrs.iter().filter(|a| {
            a.operational
                && !a.loopback
                && !a.disconnected_or_deleted
                && !a.ip.is_unspecified()
                && !a.ip.is_loopback()
                && (allow_link_local || !a.ip.is_link_local())
        });
        let first = eligible.clone().next()?;
        eligible.find(|a| a.primary).or(Some(first)).map(|a| a.ip)
    };
    pick(false).or_else(|| pick(true))
}

/// Reads the active local interface IPv4 address via IP Helper without opening any socket (R-010).
/// Uses `GetIpAddrTable` + `GetIfEntry` instead of `GetAdaptersAddresses` because `Win32_Networking_WinSock`
/// is banned by `deny.toml`.
pub fn local_ip() -> io::Result<Option<String>> {
    let mut size = 0u32;
    let mut buf = Vec::<u32>::new();
    for _ in 0..8 {
        buf.resize((size as usize).div_ceil(4).max(1), 0);
        let mut given = size;
        let ptr = (size > 0).then(|| buf.as_mut_ptr().cast::<MIB_IPADDRTABLE>());
        // SAFETY: passes null on the size query or a `u32`-aligned buffer of `given` bytes.
        let code = unsafe { GetIpAddrTable(ptr, &mut given, false) };
        if code == NO_ERROR.0 && size > 0 {
            let byte_len = buf.len() * 4;
            let header = std::mem::offset_of!(MIB_IPADDRTABLE, table);
            let max_rows =
                byte_len.saturating_sub(header) / std::mem::size_of::<MIB_IPADDRROW_XP>();
            // SAFETY: `buf` holds a valid `MIB_IPADDRTABLE` written by `GetIpAddrTable` and `count`
            // is bounded by the allocated buffer size.
            let rows = unsafe {
                let table = &*buf.as_ptr().cast::<MIB_IPADDRTABLE>();
                let count = (table.dwNumEntries as usize).min(max_rows);
                std::slice::from_raw_parts(table.table.as_ptr(), count)
            };
            let addrs: Vec<LocalAddr> = rows
                .iter()
                .map(|row| {
                    let mut if_row = MIB_IFROW {
                        dwIndex: row.dwIndex,
                        ..Default::default()
                    };
                    // SAFETY: `if_row.dwIndex` is set and `if_row` is a live `MIB_IFROW` on the stack.
                    let operational = unsafe { GetIfEntry(&mut if_row) } == NO_ERROR.0
                        && if_row.dwOperStatus == IF_OPER_STATUS_OPERATIONAL;
                    LocalAddr {
                        ip: Ipv4Addr::from(row.dwAddr.to_ne_bytes()),
                        operational,
                        loopback: if_row.dwType == IF_TYPE_SOFTWARE_LOOPBACK,
                        primary: (u32::from(row.wType) & MIB_IPADDR_PRIMARY) != 0,
                        disconnected_or_deleted: (u32::from(row.wType)
                            & (MIB_IPADDR_DISCONNECTED | MIB_IPADDR_DELETED))
                            != 0,
                    }
                })
                .collect();
            return Ok(select_local_ip(&addrs).map(|ip| ip.to_string()));
        }
        if code == ERROR_NO_DATA.0 {
            return Ok(None);
        }
        if code == NO_ERROR.0 || code == ERROR_INSUFFICIENT_BUFFER.0 {
            size = given.saturating_add(256);
            continue;
        }
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    Err(io::Error::other("the IP address table kept growing"))
}

pub fn restart() -> io::Result<()> {
    exit_windows(EWX_REBOOT)
}

pub fn shut_down() -> io::Result<()> {
    exit_windows(EWX_SHUTDOWN)
}

pub fn sign_out() -> io::Result<()> {
    exit_windows(EWX_LOGOFF)
}

/// Not forced: an app that blocks the end of the session still gets to ask, as in the Start menu.
fn exit_windows(how: EXIT_WINDOWS_FLAGS) -> io::Result<()> {
    // Enabling a privilege this user already holds is not elevation; the token stays medium.
    enable_shutdown_privilege()?;
    // SAFETY: no pointers; the reason code 0 is "other".
    Ok(unsafe { ExitWindowsEx(how, SHUTDOWN_REASON(0)) }?)
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful OpenProcessToken and is closed once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn enable_shutdown_privilege() -> Result<()> {
    let mut token = HANDLE::default();
    // SAFETY: `token` receives a new handle that `Handle` closes.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )?
    };
    let token = Handle(token);
    let mut luid = LUID::default();
    // SAFETY: looks up a well-known privilege name; `luid` receives the id.
    unsafe { LookupPrivilegeValueW(None, SE_SHUTDOWN_NAME, &mut luid)? };
    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    // SAFETY: `privileges` is one fully initialised entry and outlives the call.
    unsafe { AdjustTokenPrivileges(token.0, false, Some(&privileges), 0, None, None)? };
    // The call succeeds even when the privilege is not held; only the last error says so.
    // SAFETY: reads this thread's last error code.
    if unsafe { GetLastError() } == ERROR_NOT_ALL_ASSIGNED {
        return Err(Error::from(ERROR_NOT_ALL_ASSIGNED));
    }
    Ok(())
}

pub fn empty_recycle_bin() -> io::Result<()> {
    // SAFETY: no window, no path (every drive); the flags silence the dialogs, which the second
    // Enter replaced.
    let r = unsafe {
        SHEmptyRecycleBinW(
            None,
            None,
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    // An empty bin answers E_UNEXPECTED: nothing to do is a success.
    match r {
        Err(e) if e.code() != E_UNEXPECTED => Err(e.into()),
        _ => Ok(()),
    }
}

/// The next `AppsUseLightTheme` for the current one: light (1) becomes dark (0), anything else light.
pub fn toggled(light: Option<u32>) -> u32 {
    u32::from(light == Some(0))
}

/// Flips Windows between light and dark for apps and the system, then tells every window.
pub fn toggle_dark_mode() -> io::Result<()> {
    let next = toggled(read_dword(APPS_VALUE));
    write_dword(APPS_VALUE, next)?;
    write_dword(SYSTEM_VALUE, next)?;
    broadcast_theme_change();
    Ok(())
}

fn broadcast_theme_change() {
    // SAFETY: the broadcast string is a static, NUL-terminated wide literal; the call waits at most
    // half a second per window and skips hung ones.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            500,
            None,
        );
    }
}

/// `SystemUsesLightTheme` as stored, or None when the value is missing.
pub fn system_uses_light_theme() -> Option<u32> {
    read_dword(SYSTEM_VALUE)
}

/// Writes both values back as read before a change (a missing one is left alone), then tells every
/// window. For tests that toggle the theme.
pub fn restore_theme(apps: Option<u32>, system: Option<u32>) {
    for (name, value) in [(APPS_VALUE, apps), (SYSTEM_VALUE, system)] {
        if let Some(v) = value {
            let _ = write_dword(name, v);
        }
    }
    broadcast_theme_change();
}

/// `AppsUseLightTheme` as stored, or None when the value is missing.
pub fn apps_use_light_theme() -> Option<u32> {
    read_dword(APPS_VALUE)
}

fn read_dword(name: &str) -> Option<u32> {
    let (key, name) = (HSTRING::from(PERSONALIZE), HSTRING::from(name));
    let mut data = 0u32;
    let mut len = 4u32;
    // SAFETY: a DWORD query writes at most 4 bytes into `data`, which lives for the call.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut len),
        )
    };
    err.is_ok().then_some(data)
}

fn write_dword(name: &str, value: u32) -> Result<()> {
    let (key, name) = (HSTRING::from(PERSONALIZE), HSTRING::from(name));
    // SAFETY: the data pointer covers the 4 bytes of `value`, which outlives the call.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            REG_DWORD.0,
            Some(&value as *const u32 as *const c_void),
            4,
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @R-020
    #[test]
    fn r020_toggle_dark_mode_flips_app_theme() {
        assert_eq!(toggled(Some(1)), 0, "light becomes dark");
        assert_eq!(toggled(Some(0)), 1, "dark becomes light");
        assert_eq!(toggled(None), 0, "a missing value is the default, light");
    }

    // @R-010
    #[test]
    fn ticket8_select_local_ip_filters_inactive_loopback_and_prefers_primary() {
        use std::net::Ipv4Addr;

        let addr =
            |ip: [u8; 4], operational: bool, loopback: bool, primary: bool, dead: bool| LocalAddr {
                ip: Ipv4Addr::from(ip),
                operational,
                loopback,
                primary,
                disconnected_or_deleted: dead,
            };

        assert_eq!(select_local_ip(&[]), None);
        assert_eq!(
            select_local_ip(&[
                addr([127, 0, 0, 1], true, true, true, false),
                addr([0, 0, 0, 0], true, false, true, false),
                addr([10, 0, 0, 2], false, false, true, false),
                addr([10, 0, 0, 3], true, false, true, true),
            ]),
            None
        );
        assert_eq!(
            select_local_ip(&[
                addr([169, 254, 1, 5], true, false, true, false),
                addr([192, 168, 1, 20], true, false, false, false),
                addr([10, 0, 0, 5], true, false, true, false),
            ]),
            Some(Ipv4Addr::new(10, 0, 0, 5))
        );
        assert_eq!(
            select_local_ip(&[addr([169, 254, 1, 5], true, false, false, false)]),
            Some(Ipv4Addr::new(169, 254, 1, 5))
        );
    }

    // @R-010
    #[test]
    fn r010_local_ip_reads_valid_non_loopback_without_sockets() {
        let got = local_ip().unwrap_or_else(|e| panic!("local_ip: {e}"));
        if let Some(ip_str) = got {
            let ip: std::net::Ipv4Addr = ip_str
                .parse()
                .unwrap_or_else(|e| panic!("invalid IPv4 {ip_str}: {e}"));
            assert!(!ip.is_loopback() && !ip.is_unspecified());
        }
    }
}
