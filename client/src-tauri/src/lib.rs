mod identity;

use identity::{get_or_create_identity, sign_auth_challenge};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_or_create_identity,
            sign_auth_challenge
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
