//! Error types for the settings file and key chords.
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct ChordError(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsError {
    #[error("settings file is larger than 256 KiB")]
    TooLarge,
    #[error("{message}")]
    Syntax {
        line: Option<usize>,
        message: String,
    },
}
