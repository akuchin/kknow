// Without this, Windows opens a console next to the window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    kknow_desktop_lib::run()
}
