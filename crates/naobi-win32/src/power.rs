//! The Windows side of Awake (R-019, REPORT 5.4): one power request with the reason "Naobi Command
//! Awake" (so `powercfg /requests` names the app) and the two timers, all on the platform thread.
use std::ffi::c_void;
use std::sync::Mutex;
use windows::core::{w, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::System::Power::{
    CallNtPowerInformation, PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired,
    PowerRequestSystemRequired, PowerSetRequest, SystemExecutionState,
};
use windows::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

/// `AppEvent::Timer(TimerId(EXPIRY_TIMER))`: the end of a Timed mode.
pub const EXPIRY_TIMER: usize = 0xA1;
/// `AppEvent::Timer(TimerId(TICK_TIMER))`: every 60 s while Timed, to refresh the tooltip.
pub const TICK_TIMER: usize = 0xA2;
const TICK_MS: u32 = 60_000;
/// `POWER_REQUEST_CONTEXT_VERSION` (the windows crate does not export it).
const CONTEXT_VERSION: u32 = 0;
/// The longest a Windows timer can run.
const MAX_TIMER_MS: u64 = 0x7FFF_FFFF;

/// The request handle (0 for none): touched only on the platform thread, or by one test thread.
static REQUEST: Mutex<isize> = Mutex::new(0);

/// Makes, changes or releases the power request, on the calling thread. `on` holds the system awake;
/// `display` holds the display on too. False when Windows refused.
pub fn set_request_now(on: bool, display: bool) -> bool {
    let mut handle = REQUEST.lock().unwrap_or_else(|e| e.into_inner());
    if !on {
        release(&mut handle);
        return true;
    }
    if *handle == 0 {
        let context = REASON_CONTEXT {
            Version: CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: PWSTR(w!("Naobi Command Awake").as_ptr().cast_mut()),
            },
        };
        // SAFETY: `context` and its static string live for the call; the handle is closed in `release`.
        match unsafe { PowerCreateRequest(&context) } {
            Ok(h) => *handle = h.0 as isize,
            Err(_) => return false,
        }
    }
    let h = HANDLE(*handle as *mut c_void);
    // SAFETY: `h` is the request handle created above; the calls only change its request types.
    unsafe {
        let system = PowerSetRequest(h, PowerRequestSystemRequired).is_ok();
        let screen = if display {
            PowerSetRequest(h, PowerRequestDisplayRequired).is_ok()
        } else {
            let _ = PowerClearRequest(h, PowerRequestDisplayRequired);
            true
        };
        system && screen
    }
}

fn release(handle: &mut isize) {
    if *handle == 0 {
        return;
    }
    let h = HANDLE(*handle as *mut c_void);
    // SAFETY: the request handle created in `set_request_now`, cleared and closed exactly once.
    unsafe {
        let _ = PowerClearRequest(h, PowerRequestSystemRequired);
        let _ = PowerClearRequest(h, PowerRequestDisplayRequired);
        let _ = CloseHandle(h);
    }
    *handle = 0;
}

/// The `SystemExecutionState` flags now (`ES_SYSTEM_REQUIRED` is 1, `ES_DISPLAY_REQUIRED` is 2),
/// for the tests: it shows requests without an elevated `powercfg`.
pub fn execution_state() -> u32 {
    let mut state = 0u32;
    // SAFETY: writes one u32 into `state`; no input buffer.
    let _ = unsafe {
        CallNtPowerInformation(
            SystemExecutionState,
            None,
            0,
            Some(&mut state as *mut u32 as *mut c_void),
            std::mem::size_of::<u32>() as u32,
        )
    };
    state
}

/// Arms the expiry timer and the tooltip tick (`Some`), or stops both (`None`), on the platform
/// thread. Any thread may call it; the timers belong to the platform window.
pub fn timers(expiry_ms: Option<u64>) -> bool {
    let job = move || {
        let hwnd = HWND(crate::platform::p1_hwnd() as *mut c_void);
        // SAFETY: timers of the platform window, set and killed on its own thread.
        unsafe {
            let _ = KillTimer(Some(hwnd), EXPIRY_TIMER);
            let _ = KillTimer(Some(hwnd), TICK_TIMER);
            if let Some(ms) = expiry_ms {
                SetTimer(
                    Some(hwnd),
                    EXPIRY_TIMER,
                    ms.clamp(1, MAX_TIMER_MS) as u32,
                    None,
                );
                SetTimer(Some(hwnd), TICK_TIMER, TICK_MS, None);
            }
        }
    };
    crate::platform::post_to_p1(Box::new(job))
}

/// Holds or releases the request on the platform thread.
pub fn set_request(on: bool, display: bool) -> bool {
    crate::platform::post_to_p1(Box::new(move || {
        set_request_now(on, display);
    }))
}
