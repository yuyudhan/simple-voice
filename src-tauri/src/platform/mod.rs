// FilePath: src-tauri/src/platform/mod.rs
//! Desktop integration (macOS and Linux): windows, tray, Dock, login item, global shortcuts, the
//! secret store and the engine helper process.

pub(crate) mod dock;
pub(crate) mod engine_process;
pub(crate) mod keystore;
pub(crate) mod login_item;
pub(crate) mod overlay;
pub(crate) mod shortcuts;
pub(crate) mod tray;
pub(crate) mod windows;
