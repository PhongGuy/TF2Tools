use serde::{Deserialize, Serialize};

use crate::DB;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    custom_folder_path: String,
}

// There will only be one user so 1 set of settings in the settings table
impl Settings {
    async fn get_settings() -> Settings {
        let my_settings: Option<Settings> = DB
            .select(("settings", "0"))
            .await
            .expect("failed to get settings");

        if let Some(settings) = my_settings {
            println!("Settings: {:?}", settings);
            settings
        } else {
            println!("No settings found, creating new settings");
            let new_settings = Settings::default();
            let my_settings: Option<Settings> = DB
                .update(("settings", "0"))
                .content(new_settings)
                .await
                .expect("failed to create new settings");
            my_settings.unwrap()
        }
    }
    async fn save_settings(&self) -> bool {
        let is_ok: Option<Settings> = DB
            .update(("settings", "0"))
            .content(self)
            .await
            .expect("failed to save settings");

        is_ok.is_some()
    }
}

#[tauri::command]
pub async fn get_settings() -> Settings {
    Settings::get_settings().await
}

#[tauri::command]
pub async fn save_settings(settings: Settings) {
    settings.save_settings().await;
}
