//! Safe wrappers over Win32 (EX-001). Every `unsafe` block carries `// SAFETY:`.
pub mod autostart;
pub mod clipboard_marker;
pub mod dll_hardening;
pub mod dwm;
pub mod hotkeys;
#[cfg(feature = "perf")]
pub mod inject;
pub mod instance;
pub mod platform;
#[cfg(feature = "perf")]
pub mod probe;
pub mod process;
pub mod random;
#[cfg(feature = "perf")]
pub mod subclass;
pub mod token;
pub mod tray;
