//! Shared kernel for Naobi Command: ids, items, actions and the UI data types.
//! Later items add modules; this crate never names a UI framework.
#![forbid(unsafe_code)]
pub mod chord;
pub mod cipher;
pub mod color;
pub mod command;
pub mod db;
pub mod effect;
pub mod error;
pub mod event;
pub mod ids;
pub mod item;
pub mod notice;
pub mod paths;
pub mod quicklink;
pub mod settings;
pub mod snippet;
pub mod theme;
pub mod theme_file;
pub mod tokens;
pub mod ui;
pub mod view_source;
