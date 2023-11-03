// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use directories::ProjectDirs;
use once_cell::sync::Lazy;
use surrealdb::{
    engine::local::{Db, File},
    Surreal,
};

pub static DB: Lazy<Surreal<Db>> = Lazy::new(Surreal::init);
pub static PROJECT_LOCAL_DIR: Lazy<std::path::PathBuf> = Lazy::new(|| {
    ProjectDirs::from("Project", "PhongGuy", "tf2-tools")
        .expect("failed to get project directories")
        .data_local_dir()
        .to_path_buf()
});

async fn init_db() -> surrealdb::Result<()> {
    let path = PROJECT_LOCAL_DIR.join("db.db");

    // Connect to the database
    DB.connect::<File>(path).await?;
    // Select a namespace + database
    DB.use_ns("tf2-tools").use_db("tf2-tools").await
}

#[tokio::main]
async fn main() {
    // Initialize the database
    init_db().await.expect("failed to initialize database");

    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
