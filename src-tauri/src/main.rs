// Release builds are desktop GUI applications.  Keep the debug console so
// developers can still see `cargo tauri dev` diagnostics on Windows.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    ccm_reborn_lib::run();
}
