// Hide the console window on Windows release builds. No-op elsewhere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    secunit_gui::run();
}
