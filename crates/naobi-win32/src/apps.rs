//! The installed apps (REPORT 5.4): the children of the shell's `AppsFolder`, which lists Win32
//! shortcuts and packaged apps alike. An entry launches as `shell:AppsFolder\<id>`.
use windows::core::{Result, PWSTR};
use windows::Win32::System::Com::COINIT_APARTMENTTHREADED;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, SHGetKnownFolderItem,
    KF_FLAG_DEFAULT, SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
};

/// One app. `id` is what follows `shell:AppsFolder\` to start it (an AUMID, or a path-like name);
/// it is stable across scans, so frecency can key on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
}

/// Scans `AppsFolder`. Call it from a worker thread: a cold scan takes hundreds of milliseconds, and
/// the thread gets its own COM apartment for the call.
pub fn list() -> Result<Vec<AppEntry>> {
    // SAFETY: COM is initialised and released on this thread around the scan.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
    let apps = scan();
    if com {
        // SAFETY: pairs the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
    apps
}

fn scan() -> Result<Vec<AppEntry>> {
    // SAFETY: a known-folder id constant and no token; the result is an owned COM interface.
    let folder: IShellItem =
        unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)? };
    // SAFETY: binds the folder's enumerator; no bind context.
    let items: IEnumShellItems = unsafe { folder.BindToHandler(None, &BHID_EnumItems)? };
    let mut apps = Vec::new();
    loop {
        let mut slot = [None];
        let mut fetched = 0u32;
        // SAFETY: `slot` holds one interface slot and `fetched` receives the count.
        unsafe { items.Next(&mut slot, Some(&mut fetched))? };
        let Some(item) = slot[0].take().filter(|_| fetched == 1) else {
            break;
        };
        // An entry the shell cannot name is skipped: it could not be shown or launched.
        if let (Some(name), Some(id)) = (
            name_of(&item, SIGDN_NORMALDISPLAY),
            name_of(&item, SIGDN_PARENTRELATIVEPARSING),
        ) {
            apps.push(AppEntry { id, name });
        }
    }
    Ok(apps)
}

fn name_of(item: &IShellItem, kind: SIGDN) -> Option<String> {
    // SAFETY: returns a CoTaskMem string that is freed below, once, after it is copied.
    let p: PWSTR = unsafe { item.GetDisplayName(kind).ok()? };
    // SAFETY: `p` is a valid, NUL-terminated string until it is freed.
    let s = unsafe { p.to_string().ok() };
    // SAFETY: frees the string GetDisplayName allocated; `p` is not used afterwards.
    unsafe { CoTaskMemFree(Some(p.0.cast_const().cast())) };
    s.filter(|s| !s.is_empty())
}
