// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    std::panic::set_hook(Box::new(|panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = panic_info
            .location()
            .map(|loc| format!("at {}:{}", loc.file(), loc.line()))
            .unwrap_or_else(|| "unknown location".to_string());

        let error_msg = format!("ProxiShare Backend Panic!\n\nMessage: {}\nLocation: {}\n\nPlease report this issue on GitHub.", message, location);

        // Final attempt to show a message box if possible
        eprintln!("{}", error_msg);
    }));

    tauri_app_lib::run()
}
