//! Application layer: state and commands behind the Tauri IPC boundary.
//! Every command is a plain function so it can be tested without Tauri.

pub mod admin;
pub mod connect;
pub mod error;
pub mod inventory;
pub mod sftp;
pub mod state;
pub mod tools;

pub use error::ApiError;
pub use state::{App, AppEvent};
