//! The Windows Search side of the Files section (R-013, REPORT 5.4): the state of the `WSearch`
//! service, and one SQL query against the system index over OLE DB (`Search.CollatorDSO`). Loading
//! OLE DB maps tens of megabytes that never leave, so the query runs only in the app's transient
//! `--search-files` child; the resident process calls just `service_running`.
use std::ffi::c_void;
use windows::core::{w, Interface, GUID, PCWSTR};
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Search::{
    IAccessor, ICommandText, IDBCreateCommand, IDBCreateSession, IDBInitialize, IRowset,
    DBACCESSOR_ROWDATA, DBBINDING, DBMEMOWNER_PROVIDEROWNED, DBPART_STATUS, DBPART_VALUE,
    DBTYPE_BYREF, DBTYPE_WSTR, HACCESSOR,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_MANAGER_CONNECT,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
};

/// `DBGUID_DEFAULT`: the provider's default command dialect.
const DBGUID_DEFAULT: GUID = GUID::from_u128(0xC8B521FB_5CF3_11CE_ADE5_00AA0044773D);
/// Rows asked from the provider at a time.
const BATCH: usize = 16;
/// `DBSTATUS_S_OK`.
const STATUS_OK: u32 = 0;

/// True while the Windows Search service runs. A service that is stopped, starting, disabled or
/// missing cannot answer a query.
pub fn service_running() -> bool {
    // SAFETY: the handles are opened, read and closed here; the status struct is a local.
    unsafe {
        let Ok(scm) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) else {
            return false;
        };
        let mut running = false;
        if let Ok(service) = OpenServiceW(scm, w!("WSearch"), SERVICE_QUERY_STATUS) {
            let mut status = SERVICE_STATUS::default();
            running = QueryServiceStatus(service, &mut status).is_ok()
                && status.dwCurrentState == SERVICE_RUNNING;
            let _ = CloseServiceHandle(service);
        }
        let _ = CloseServiceHandle(scm);
        running
    }
}

/// Why a query gave no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchError {
    /// The provider or the index could not be opened.
    Unavailable,
    /// The query itself failed.
    Failed,
}

/// The row layout the accessor reads into: a status, then a pointer the provider owns.
#[repr(C)]
struct Row {
    status: u32,
    path: *const u16,
}

/// Runs `sql` (one column: the item's path) and returns the paths. Call it from a thread of its
/// own: it initialises COM for the call.
pub fn query(sql: &str) -> Result<Vec<String>, SearchError> {
    // SAFETY: COM is initialised and released on this thread around the query.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
    let rows = run(sql);
    if com {
        // SAFETY: pairs the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
    rows
}

fn run(sql: &str) -> Result<Vec<String>, SearchError> {
    // SAFETY: every interface here is an owned COM pointer released on drop; the row buffers and the
    // binding live for the calls that use them, and a provider-owned string is copied before its
    // row is released.
    unsafe {
        let clsid =
            CLSIDFromProgID(w!("Search.CollatorDSO.1")).map_err(|_| SearchError::Unavailable)?;
        let source: IDBInitialize = CoCreateInstance(&clsid, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| SearchError::Unavailable)?;
        source.Initialize().map_err(|_| SearchError::Unavailable)?;
        let session: IDBCreateSession = source.cast().map_err(|_| SearchError::Unavailable)?;
        let create: IDBCreateCommand = session
            .CreateSession(None, &IDBCreateCommand::IID)
            .and_then(|u| u.cast())
            .map_err(|_| SearchError::Unavailable)?;
        let command: ICommandText = create
            .CreateCommand(None, &ICommandText::IID)
            .and_then(|u| u.cast())
            .map_err(|_| SearchError::Failed)?;
        let text: Vec<u16> = sql.encode_utf16().chain(std::iter::once(0)).collect();
        command
            .SetCommandText(&DBGUID_DEFAULT, PCWSTR(text.as_ptr()))
            .map_err(|_| SearchError::Failed)?;
        let mut rowset: Option<windows::core::IUnknown> = None;
        command
            .Execute(None, &IRowset::IID, None, None, Some(&mut rowset))
            .map_err(|_| SearchError::Failed)?;
        let rowset: IRowset = rowset
            .ok_or(SearchError::Failed)?
            .cast()
            .map_err(|_| SearchError::Failed)?;
        read(&rowset)
    }
}

/// Reads every row of `rowset` (the column is bound by reference, so the strings stay the provider's
/// until the row is released).
///
/// # Safety
/// `rowset` is a live rowset whose first column is a string.
unsafe fn read(rowset: &IRowset) -> Result<Vec<String>, SearchError> {
    // SAFETY: see the function's contract; the accessor is released before returning.
    unsafe {
        let accessor: IAccessor = rowset.cast().map_err(|_| SearchError::Failed)?;
        let binding = DBBINDING {
            iOrdinal: 1,
            obValue: std::mem::offset_of!(Row, path),
            obLength: 0,
            obStatus: std::mem::offset_of!(Row, status),
            pTypeInfo: std::mem::ManuallyDrop::new(None),
            pObject: std::ptr::null_mut(),
            pBindExt: std::ptr::null_mut(),
            dwPart: (DBPART_VALUE.0 | DBPART_STATUS.0) as u32,
            dwMemOwner: DBMEMOWNER_PROVIDEROWNED.0 as u32,
            eParamIO: 0,
            cbMaxLen: 0,
            dwFlags: 0,
            wType: (DBTYPE_WSTR.0 | DBTYPE_BYREF.0) as u16,
            bPrecision: 0,
            bScale: 0,
        };
        let mut handle = HACCESSOR::default();
        accessor
            .CreateAccessor(
                DBACCESSOR_ROWDATA.0 as u32,
                1,
                &binding,
                std::mem::size_of::<Row>(),
                &mut handle,
                None,
            )
            .map_err(|_| SearchError::Failed)?;
        let mut paths = Vec::new();
        let mut outcome = Ok(());
        'batches: loop {
            // `HROW**` in C: a null first element lets the provider allocate the handle array and
            // put its address there; it is freed below.
            let mut slot = [std::ptr::null_mut::<usize>(); BATCH];
            let mut got = 0usize;
            // DB_S_ENDOFROWSET is a success code: `got` is 0 then.
            if rowset.GetNextRows(0, 0, &mut got, &mut slot).is_err() {
                outcome = Err(SearchError::Failed);
                break;
            }
            let handles = slot[0];
            if got == 0 || handles.is_null() {
                if !handles.is_null() {
                    CoTaskMemFree(Some(handles.cast()));
                }
                break;
            }
            let mut buf = Row {
                status: 0,
                path: std::ptr::null(),
            };
            for i in 0..got {
                let row = *handles.add(i);
                let ok = rowset
                    .GetData(row, handle, &mut buf as *mut Row as *mut c_void)
                    .is_ok();
                if ok && buf.status == STATUS_OK && !buf.path.is_null() {
                    let mut len = 0;
                    while *buf.path.add(len) != 0 {
                        len += 1;
                    }
                    paths.push(String::from_utf16_lossy(std::slice::from_raw_parts(
                        buf.path, len,
                    )));
                }
                if !ok {
                    outcome = Err(SearchError::Failed);
                }
            }
            let _ = rowset.ReleaseRows(
                got,
                handles.cast_const(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            CoTaskMemFree(Some(handles.cast()));
            if outcome.is_err() {
                break 'batches;
            }
        }
        let _ = accessor.ReleaseAccessor(handle, None);
        outcome.map(|()| paths)
    }
}
