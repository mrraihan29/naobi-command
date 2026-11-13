//! Clipboard history (R-009, R-015, D-014, D-015): what is allowed into the store, how it is encrypted,
//! bounded and purged. Pure logic over the kernel `Db` and three ports (cipher, clock, clipboard
//! source); the Windows side lives in naobi-win32.
#![forbid(unsafe_code)]
pub mod capture;
pub mod filters;
pub mod history;
pub mod image;
pub mod limits;
pub mod ports;
pub mod testing;

pub use history::{Error, History, Purged, Stored};
pub use ports::{Captured, Cipher, ClipSource, Clock, Content, Flags};
