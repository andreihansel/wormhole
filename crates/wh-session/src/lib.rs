//! Terminal sessions: lifecycle, output fan-out, broadcast input, logs.

pub mod backends;
pub mod manager;

pub use manager::{CloseReason, SessionEvent, SessionId, SessionManager, SessionOptions, TermInput, TermOutput, TerminalIo};
