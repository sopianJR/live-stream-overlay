// Mencegah jendela konsol tambahan muncul di build release Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    livestream_overlay_lib::run()
}
