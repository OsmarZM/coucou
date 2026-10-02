// Coucou runs without a console window: Mochi is the whole UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = coucou_lib::documents::worker_entry() {
        std::process::exit(code);
    }
    coucou_lib::run()
}
