// Coucou runs without a console window: Mochi is the whole UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|arg| {
        matches!(
            arg.as_str(),
            "--managed-free-service" | "--hermes-tool-bridge"
        )
    }) {
        let bridge = std::env::args().any(|arg| arg == "--hermes-tool-bridge");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Headless runtime unavailable");
        let result = runtime.block_on(async {
            if bridge {
                coucou_lib::hermes_bridge::worker_entry().await
            } else {
                coucou_lib::managed_services::headless_entry().await
            }
        });
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if let Some(code) = coucou_lib::documents::worker_entry() {
        std::process::exit(code);
    }
    coucou_lib::run()
}
