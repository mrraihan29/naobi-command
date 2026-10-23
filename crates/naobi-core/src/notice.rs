//! A short message shown to the Owner. Never carries clipboard content or query text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub subject: String,
    pub message: String,
    pub kind: NoticeKind,
}
