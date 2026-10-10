// FilePath: mobile/plugins/sv-native/build.rs
#![forbid(unsafe_code)]

// No commands: the web view never calls this plugin; only the app's Rust code does.
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
