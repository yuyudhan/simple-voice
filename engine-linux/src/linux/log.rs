// FilePath: engine-linux/src/linux/log.rs
//! Logs go to stderr only, in the Swift helper's `[level] message` form: the app keeps the last
//! lines and shows an `[error]` line as the reason when the helper exits.

pub(crate) fn info(message: &str) {
    eprintln!("[info] {message}");
}

pub(crate) fn error(message: &str) {
    eprintln!("[error] {message}");
}
