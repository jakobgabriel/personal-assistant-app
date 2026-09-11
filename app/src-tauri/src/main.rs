// Kein Konsolenfenster neben der App unter Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tory_lib::run()
}
