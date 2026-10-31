//! The one port the engine needs from its host. The production clock is supplied by the controller.
pub trait Clock {
    /// Unix milliseconds.
    fn now_ms(&self) -> i64;
}
