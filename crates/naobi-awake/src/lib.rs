//! Awake (R-019, D-024): keep the PC from sleeping, forever or for a while, optionally with the
//! display on. The state machine and the command grammar are here; Windows (the power request, the
//! timers, the tray) is behind `AwakePort`.
#![forbid(unsafe_code)]

pub mod commands;
pub mod grammar;
pub mod state;

pub use commands::AwakeCommands;
pub use grammar::{parse, Cmd};
pub use state::{Awake, AwakePort, Kind, Mode, View};
