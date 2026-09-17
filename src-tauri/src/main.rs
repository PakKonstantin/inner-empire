// On Windows, a release build must not open a console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    inner_empire_lib::run();
}
