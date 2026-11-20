//! Platform thread (P1, REPORT 3.5): a hidden top-level tool window that owns the hotkeys and the tray
//! icon. It forwards each listed message as an `AppEvent` and runs the jobs posted to it. It does
//! nothing else (T-021). A hidden top-level window gets broadcasts; a message-only window does not (D-035).
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{mpsc, Mutex, MutexGuard};

use naobi_core::event::AppEvent;
use naobi_core::ids::TimerId;
use naobi_core::settings::Settings;
use windows::core::{w, Error, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, KillTimer, PostMessageW,
    RegisterClassW, RegisterWindowMessageW, SetTimer, MSG, PBT_APMRESUMEAUTOMATIC,
    PBT_APMRESUMESUSPEND, SW_SHOWNORMAL, WINDOW_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WM_CONTEXTMENU,
    WM_DISPLAYCHANGE, WM_HOTKEY, WM_POWERBROADCAST, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMER,
    WNDCLASSW, WS_EX_TOOLWINDOW,
};

use crate::hotkeys::{self, Registry, Win32};
use crate::instance;
use crate::tray::{self, MenuChoice, Overlay, TrayState};

/// Wake message for the job queue. It carries no data (T-002): P1 reads the queue, never the message.
const WM_APP_POST: u32 = WM_APP + 3;

type Job = Box<dyn FnOnce() + Send>;

struct Callbacks {
    event: Box<dyn Fn(AppEvent) + Send + Sync>,
    quit: Box<dyn Fn() + Send + Sync>,
}

static CALLBACKS: std::sync::OnceLock<Callbacks> = std::sync::OnceLock::new();
/// The platform window as a plain value; 0 until P1 is up.
static P1: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// Hotkey registrations; only the platform thread touches them (through `apply_pause` and friends).
static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);
/// Jobs waiting for P1, in posting order. Any thread pushes; P1 pops on each wake.
static JOBS: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());

fn hwnd_of(v: isize) -> HWND {
    HWND(v as *mut c_void)
}

fn lock_registry() -> MutexGuard<'static, Option<Registry>> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

fn enqueue(job: Job) {
    JOBS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .push_back(job);
}

fn next_job() -> Option<Job> {
    JOBS.lock().unwrap_or_else(|e| e.into_inner()).pop_front()
}

/// Monotonic timestamp (QueryPerformanceCounter ticks), the clock of the readiness lines.
pub fn qpc() -> i64 {
    let mut t = 0i64;
    // SAFETY: writes one i64 into `t`.
    let _ = unsafe { QueryPerformanceCounter(&mut t) };
    t
}

/// Milliseconds in `ticks` QueryPerformanceCounter ticks.
pub fn qpc_to_ms(ticks: i64) -> f64 {
    let mut f = 0i64;
    // SAFETY: writes one i64 into `f`.
    let _ = unsafe { QueryPerformanceFrequency(&mut f) };
    ticks as f64 * 1000.0 / f.max(1) as f64
}

/// The platform window's handle; constant for the life of the process. 0 before P1 is up.
pub fn p1_hwnd() -> isize {
    P1.load(Ordering::SeqCst)
}

/// Maps a forwarded message to its event (T-021: P1 forwards only). None for anything else.
pub fn forward(m: u32, w: WPARAM, _l: LPARAM) -> Option<AppEvent> {
    match m {
        WM_HOTKEY => hotkeys::name_for_id(w.0).map(AppEvent::Hotkey),
        WM_CLIPBOARDUPDATE => {
            // SAFETY: reads the clipboard sequence counter; no pointers.
            Some(AppEvent::ClipboardChanged(unsafe {
                GetClipboardSequenceNumber()
            }))
        }
        WM_SETTINGCHANGE => Some(AppEvent::ThemeChanged),
        WM_POWERBROADCAST if is_resume(w.0) => Some(AppEvent::Resume),
        WM_DISPLAYCHANGE => Some(AppEvent::DisplayChanged),
        WM_TIMER => Some(AppEvent::Timer(TimerId(w.0))),
        instance::WM_APP_SHOW => Some(AppEvent::ShowRequested),
        _ => None,
    }
}

/// True for the two power events that mean the machine woke up.
pub fn is_resume(wparam: usize) -> bool {
    wparam == PBT_APMRESUMEAUTOMATIC as usize || wparam == PBT_APMRESUMESUSPEND as usize
}

fn emit(ev: AppEvent) {
    if let Some(c) = CALLBACKS.get() {
        (c.event)(ev);
    }
}

/// Queues `job` and wakes P1, which runs it on the platform thread in posting order. The wake carries
/// no data, so a forged wake can only run jobs that are already queued (T-002). Returns false when P1
/// is not up; a failed wake leaves the job queued for the next wake.
pub fn post_to_p1(job: Job) -> bool {
    let hwnd = p1_hwnd();
    if hwnd == 0 {
        return false;
    }
    enqueue(job);
    // SAFETY: posts a message with no pointers or data to our own window.
    unsafe { PostMessageW(Some(hwnd_of(hwnd)), WM_APP_POST, WPARAM(0), LPARAM(0)) }.is_ok()
}

/// Pauses or resumes every global hotkey (R-022). Any thread may call it.
pub fn set_hotkeys_paused(paused: bool) -> bool {
    post_to_p1(Box::new(move || apply_pause(paused)))
}

/// Arms (`Some(ms)`) or stops (`None`) a timer on the platform window.
pub fn set_timer(id: usize, ms: Option<u64>) -> bool {
    const MAX_TIMER_MS: u64 = 0x7FFF_FFFF;
    post_to_p1(Box::new(move || {
        let hwnd = hwnd_of(p1_hwnd());
        // SAFETY: timers belong to the platform window and are updated on its own thread.
        unsafe {
            let _ = KillTimer(Some(hwnd), id);
            if let Some(ms) = ms {
                let _ = SetTimer(Some(hwnd), id, ms.clamp(1, MAX_TIMER_MS) as u32, None);
            }
        }
    }))
}

/// Removes the tray icon and ends the app, as the tray's Quit does. Any thread may call it.
pub fn quit() -> bool {
    post_to_p1(Box::new(|| {
        tray::remove(hwnd_of(p1_hwnd()));
        if let Some(c) = CALLBACKS.get() {
            (c.quit)();
        }
    }))
}

/// Replaces the tray icon state. Any thread may call it. While hotkeys are paused the overlay stays
/// Paused, so a state update (Awake) cannot hide the pause or flip the menu check.
pub fn set_tray_state(mut state: TrayState) -> bool {
    post_to_p1(Box::new(move || {
        if is_paused() {
            state.overlay = Overlay::Paused;
        } else if state.overlay == Overlay::Paused {
            state.overlay = Overlay::None;
        }
        tray::set_state(hwnd_of(p1_hwnd()), state)
    }))
}

fn is_paused() -> bool {
    lock_registry().as_ref().is_some_and(Registry::is_paused)
}

/// Runs one registration pass, then reports the refused chords and the ones now held.
fn register_with(hwnd: HWND, pass: impl FnOnce(&mut Registry, &mut Win32) -> Vec<AppEvent>) {
    let events = match lock_registry().as_mut() {
        Some(reg) => {
            let mut events = pass(reg, &mut Win32 { hwnd });
            events.extend(reg.held(&events));
            events
        }
        None => Vec::new(),
    };
    for ev in events {
        emit(ev);
    }
}

fn apply_pause(paused: bool) {
    let hwnd = hwnd_of(p1_hwnd());
    register_with(hwnd, |reg, os| reg.set_paused(os, paused));
    tray::set_paused(hwnd, paused);
}

/// Applies new hotkey chords from a reloaded settings file (D-009). A refused chord raises the usual
/// conflict event. Any thread may call it.
pub fn reload_hotkeys(settings: naobi_core::settings::Settings) -> bool {
    post_to_p1(Box::new(move || {
        register_with(hwnd_of(p1_hwnd()), |reg, os| reg.reconfigure(os, &settings));
    }))
}

/// Retry after sleep: registers again whatever another process took meanwhile.
fn retry_registrations(hwnd: HWND) {
    register_with(hwnd, Registry::register_all);
}

/// Starts P1 with the chords of `settings` and returns once they are registered and the tray icon is
/// shown ("tray-ready"). `event` runs on P1 for each forwarded message; `quit` runs after the Owner
/// chose Quit from the tray menu.
pub fn start_with(
    settings: &Settings,
    event: impl Fn(AppEvent) + Send + Sync + 'static,
    quit: impl Fn() + Send + Sync + 'static,
) -> Result<()> {
    let callbacks = Callbacks {
        event: Box::new(event),
        quit: Box::new(quit),
    };
    if CALLBACKS.set(callbacks).is_err() {
        return Err(Error::from(E_FAIL));
    }
    let registry = Registry::new(settings);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || match init(registry) {
        Ok(()) => {
            let _ = tx.send(Ok(()));
            message_loop();
        }
        Err(e) => {
            let _ = tx.send(Err(e));
        }
    });
    rx.recv().unwrap_or_else(|_| Err(Error::from(E_FAIL)))
}

fn init(mut registry: Registry) -> Result<()> {
    // SAFETY: registers a class and creates one hidden window on this thread; the class name is static.
    let hwnd = unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: instance::CLASS,
            ..Default::default()
        };
        RegisterClassW(&class);
        // Never shown: a hidden top-level tool window, so it is not on the taskbar or in Alt+Tab (R-022).
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            instance::CLASS,
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )?
    };
    P1.store(hwnd.0 as isize, Ordering::SeqCst);
    // SAFETY: a registered message name with no pointers.
    let taskbar = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(taskbar, Ordering::SeqCst);
    let conflicts = registry.register_all(&mut Win32 { hwnd });
    *lock_registry() = Some(registry);
    for ev in conflicts {
        emit(ev);
    }
    // A failed add (the taskbar may not exist yet at logon) must not stop P1: TaskbarCreated adds it later.
    let _ = tray::add(hwnd);
    Ok(())
}

fn message_loop() {
    let mut msg = MSG::default();
    // SAFETY: the standard pump, on the thread that created the window.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        // SAFETY: `msg` was filled by GetMessageW.
        unsafe { DispatchMessageW(&msg) };
    }
}

/// Runs every queued job, in order, on the platform thread. The wake message is ignored, so a forged
/// wake with garbage data only drains jobs that were queued by this process.
fn drain_jobs() {
    while let Some(job) = next_job() {
        job();
    }
}

/// Tray mouse events. `w` is the notify icon id, which must be ours (1): another process cannot pop
/// our menu and take the foreground by sending a bare callback message.
fn on_tray(h: HWND, w: WPARAM, l: LPARAM) {
    let event = (l.0 as u32) & 0xFFFF;
    if w.0 != 1 || (event != WM_RBUTTONUP && event != WM_CONTEXTMENU) {
        return;
    }
    match tray::menu(h) {
        Some(MenuChoice::Open) => emit(AppEvent::ShowRequested),
        Some(MenuChoice::Pause) => apply_pause(!is_paused()),
        Some(MenuChoice::Settings) => {
            open_settings();
        }
        Some(MenuChoice::Quit) => {
            tray::remove(h);
            if let Some(c) = CALLBACKS.get() {
                (c.quit)();
            }
        }
        None => {}
    }
}

/// Opens `settings.toml` with the Owner's default editor (the tray's Settings item). False when it
/// could not be opened.
pub fn open_settings() -> bool {
    let Some(dir) = naobi_core::paths::settings_dir() else {
        return false;
    };
    let path = dir.join("settings.toml");
    // A deleted file is made again from the default text, as at startup, instead of the shell's
    // "cannot find the file" prompt. An existing file is never touched.
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = f.write_all(naobi_core::settings::Settings::default_toml().as_bytes());
    }
    let file = HSTRING::from(path.to_string_lossy().as_ref());
    // SAFETY: `file` and the static verb outlive the call; the other pointers are null.
    let r = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &file,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    r.0 as isize > 32
}

fn handle(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
    if let Some(ev) = forward(m, w, l) {
        match ev {
            AppEvent::Resume => retry_registrations(h),
            AppEvent::ThemeChanged => tray::refresh(h),
            _ => {}
        }
        emit(ev);
        return Some(LRESULT(0));
    }
    match m {
        WM_APP_POST => {
            drain_jobs();
            Some(LRESULT(0))
        }
        tray::WM_TRAY => {
            on_tray(h, w, l);
            Some(LRESULT(0))
        }
        _ if m != 0 && m == TASKBAR_CREATED.load(Ordering::SeqCst) => {
            // Explorer restarted and dropped the icon.
            let _ = tray::add(h);
            Some(LRESULT(0))
        }
        _ => None,
    }
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match handle(h, m, w, l) {
        Some(r) => r,
        // SAFETY: forwards the unchanged arguments to the default procedure.
        None => unsafe { DefWindowProcW(h, m, w, l) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn resume_is_only_the_two_wake_events() {
        assert!(is_resume(PBT_APMRESUMEAUTOMATIC as usize));
        assert!(is_resume(PBT_APMRESUMESUSPEND as usize));
        assert!(!is_resume(0x0004)); // PBT_APMSUSPEND
    }

    #[test]
    fn wake_with_garbage_runs_only_queued_jobs_in_order() {
        let log: Arc<Mutex<Vec<u8>>> = Arc::default();
        for n in [1u8, 2, 3] {
            let log = log.clone();
            enqueue(Box::new(move || log.lock().unwrap().push(n)));
        }
        let null = HWND(std::ptr::null_mut());
        // A forged wake: data in both words must not be read as a pointer or a job.
        let garbage_w = WPARAM(usize::MAX);
        let garbage_l = LPARAM(0x7FFF_DEAD_BEEF);
        assert!(handle(null, WM_APP_POST, garbage_w, garbage_l).is_some());
        assert_eq!(*log.lock().unwrap(), vec![1, 2, 3]);
        // With nothing queued, a forged wake runs nothing.
        assert!(handle(null, WM_APP_POST, garbage_w, garbage_l).is_some());
        assert_eq!(*log.lock().unwrap(), vec![1, 2, 3]);
    }
}
