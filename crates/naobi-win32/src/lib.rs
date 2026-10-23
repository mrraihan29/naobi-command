//! Safe wrappers over Win32 (EX-001). Every `unsafe` block carries `// SAFETY:`.
pub mod dwm;
#[cfg(feature = "perf")]
pub mod inject;
pub mod platform;
pub mod process;
pub mod token;
pub mod tray;
