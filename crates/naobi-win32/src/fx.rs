//! Hardened WinHttp HTTPS fetcher for daily currency exchange rates (R-010, ADR-0008).
//! Enforces HTTPS-only (TLS 1.2+ with OS certificate validation), a hardcoded host allowlist,
//! zero cookies/tracking headers, 5 s timeouts, and a 256 KB response size cap.
use std::ffi::c_void;
use std::fmt;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts, INTERNET_DEFAULT_HTTPS_PORT,
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_DISABLE_COOKIES, WINHTTP_DISABLE_REDIRECTS,
    WINHTTP_FLAG_SECURE, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
    WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_QUERY_CONTENT_LENGTH,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

pub const TIMEOUT_MS: i32 = 5_000;
pub const MAX_RESPONSE_BYTES: usize = 262_144;

const ALLOWED_HOSTS: [&str; 2] = ["api.frankfurter.dev", "www.ecb.europa.eu"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowlistedTarget {
    pub host: &'static str,
    pub path_and_query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FxError {
    DisallowedUrl,
    ResponseTooLarge,
    HttpStatus(u32),
    InvalidUtf8,
    WinHttp(u32),
}

impl fmt::Display for FxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DisallowedUrl => f.write_str("disallowed FX rate URL"),
            Self::ResponseTooLarge => f.write_str("FX rate response exceeded 256 KB"),
            Self::HttpStatus(code) => write!(f, "FX rate server returned HTTP {code}"),
            Self::InvalidUtf8 => f.write_str("FX rate response was not valid UTF-8"),
            Self::WinHttp(code) => write!(f, "WinHttp error {code}"),
        }
    }
}

impl std::error::Error for FxError {}

/// Validates that `url` is `https://` to one of the hardcoded allowlisted FX hosts, with no
/// userinfo, custom port, fragment, or control characters.
pub fn parse_allowlisted_https_url(url: &str) -> Result<AllowlistedTarget, FxError> {
    if url
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
    {
        return Err(FxError::DisallowedUrl);
    }
    let rest = url.strip_prefix("https://").ok_or(FxError::DisallowedUrl)?;
    let (authority, path_and_query) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty()
        || authority.contains('@')
        || authority.contains(':')
        || path_and_query.contains('#')
    {
        return Err(FxError::DisallowedUrl);
    }
    let host = ALLOWED_HOSTS
        .into_iter()
        .find(|h| authority.eq_ignore_ascii_case(h))
        .ok_or(FxError::DisallowedUrl)?;
    Ok(AllowlistedTarget {
        host,
        path_and_query: path_and_query.to_owned(),
    })
}

/// Appends `chunk` to `buf`, failing with `FxError::ResponseTooLarge` if total exceeds 256 KB.
pub fn append_bounded(buf: &mut Vec<u8>, chunk: &[u8]) -> Result<(), FxError> {
    if buf.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
        return Err(FxError::ResponseTooLarge);
    }
    buf.extend_from_slice(chunk);
    Ok(())
}

struct WinHttpHandle(*mut c_void);

impl WinHttpHandle {
    fn new(raw: *mut c_void) -> Result<Self, FxError> {
        if raw.is_null() {
            Err(FxError::WinHttp(last_win32_error()))
        } else {
            Ok(Self(raw))
        }
    }
}

impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a non-null HINTERNET returned by WinHttp and closed once on drop.
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

fn last_win32_error() -> u32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
}

fn set_u32_option(handle: &WinHttpHandle, option: u32, value: u32) -> Result<(), FxError> {
    let bytes = value.to_ne_bytes();
    // SAFETY: `handle.0` is a valid WinHttp handle and `bytes` is a 4-byte DWORD option buffer.
    let ok = unsafe { WinHttpSetOption(Some(handle.0), option, Some(&bytes)) };
    ok.map_err(|_| FxError::WinHttp(last_win32_error()))
}

fn query_u32_header(request: &WinHttpHandle, flags: u32) -> Option<u32> {
    let mut val = 0u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    // SAFETY: `request.0` is a valid received request handle; `val` and `len` are initialized locals.
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            flags | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut val as *mut u32).cast::<c_void>()),
            &mut len,
            std::ptr::null_mut(),
        )
        .ok()?;
    }
    Some(val)
}

/// Performs a hardened HTTPS GET to an allowlisted FX endpoint and returns the UTF-8 response body.
pub fn fetch_https(url: &str) -> Result<String, FxError> {
    let target = parse_allowlisted_https_url(url)?;

    // SAFETY: static agent string and null proxy pointers; handle is wrapped in `WinHttpHandle`.
    let session = WinHttpHandle::new(unsafe {
        WinHttpOpen(
            w!("NaobiCommand/1.0"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })?;

    // SAFETY: `session.0` is a valid WinHttp session handle.
    unsafe { WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS) }
        .map_err(|_| FxError::WinHttp(last_win32_error()))?;

    // Enforce TLS 1.2+ (try TLS 1.2 | 1.3 first, fall back to TLS 1.2 only if TLS 1.3 flag is unsupported).
    if set_u32_option(
        &session,
        WINHTTP_OPTION_SECURE_PROTOCOLS,
        WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
    )
    .is_err()
    {
        set_u32_option(
            &session,
            WINHTTP_OPTION_SECURE_PROTOCOLS,
            WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2,
        )?;
    }

    let host_w = HSTRING::from(target.host);
    // SAFETY: `session.0` is valid and `host_w` stays alive across the call.
    let connect = WinHttpHandle::new(unsafe {
        WinHttpConnect(session.0, &host_w, INTERNET_DEFAULT_HTTPS_PORT, 0)
    })?;

    let path_w = HSTRING::from(&target.path_and_query);
    // SAFETY: `connect.0` is valid; `WINHTTP_FLAG_SECURE` enables HTTPS with OS certificate validation.
    let request = WinHttpHandle::new(unsafe {
        WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            &path_w,
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    })?;

    // Disable cookies and automatic redirects so requests stay stateless and cannot leave the allowlist.
    set_u32_option(
        &request,
        WINHTTP_OPTION_DISABLE_FEATURE,
        WINHTTP_DISABLE_COOKIES | WINHTTP_DISABLE_REDIRECTS,
    )?;

    // SAFETY: `request.0` is valid; zero additional headers and zero request body.
    unsafe { WinHttpSendRequest(request.0, None, None, 0, 0, 0) }
        .map_err(|_| FxError::WinHttp(last_win32_error()))?;

    // SAFETY: `request.0` has a sent request awaiting its response headers.
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }
        .map_err(|_| FxError::WinHttp(last_win32_error()))?;

    let status = query_u32_header(&request, WINHTTP_QUERY_STATUS_CODE)
        .ok_or_else(|| FxError::WinHttp(last_win32_error()))?;
    if status != 200 {
        return Err(FxError::HttpStatus(status));
    }

    if let Some(content_len) = query_u32_header(&request, WINHTTP_QUERY_CONTENT_LENGTH) {
        if content_len as usize > MAX_RESPONSE_BYTES {
            return Err(FxError::ResponseTooLarge);
        }
    }

    let mut body = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let mut avail = 0u32;
        // SAFETY: `request.0` is a valid response handle; `avail` is an initialized local u32.
        unsafe { WinHttpQueryDataAvailable(request.0, &mut avail) }
            .map_err(|_| FxError::WinHttp(last_win32_error()))?;
        if avail == 0 {
            break;
        }
        let to_read = (avail as usize).min(chunk.len()) as u32;
        let mut read = 0u32;
        // SAFETY: `chunk` has at least `to_read` writable bytes; `read` receives bytes read.
        unsafe {
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast::<c_void>(),
                to_read,
                &mut read,
            )
        }
        .map_err(|_| FxError::WinHttp(last_win32_error()))?;
        if read == 0 {
            break;
        }
        append_bounded(&mut body, &chunk[..read as usize])?;
    }

    String::from_utf8(body).map_err(|_| FxError::InvalidUtf8)
}
