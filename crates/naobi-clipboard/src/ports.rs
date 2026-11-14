//! The ports of the clipboard module, and the data that crosses them.
pub use naobi_core::cipher::{Cipher, CipherError};

pub trait Clock: Send {
    fn now_ms(&self) -> i64;
}

/// What the clipboard held after one change, as the Windows side read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub flags: Flags,
    /// True when the write carries the Launcher's own marker (D-019).
    pub marker: bool,
    /// The file name of the exe that owns the clipboard, such as `ProtonPass.exe`. Never a path.
    pub owner_exe: Option<String>,
    pub content: Content,
}

/// The privacy formats other programs set next to their data (R-009).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    /// `ExcludeClipboardContentFromMonitorProcessing` is present.
    pub exclude_from_monitor: bool,
    /// `CanIncludeInClipboardHistory`, when set.
    pub can_include_in_history: Option<u32>,
    /// `CanUploadToCloudClipboard`, when set.
    pub can_upload_to_cloud: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    /// A raw `CF_DIB`: a BITMAPINFOHEADER and the pixels.
    Dib(Vec<u8>),
    Files(Vec<String>),
}

/// The clipboard as the capture reads it.
pub trait ClipSource {
    /// The clipboard sequence number now.
    fn sequence(&self) -> u32;
    /// Reads text, a bitmap or a file list. None when nothing readable is there.
    fn read(&self) -> Option<Captured>;
}
