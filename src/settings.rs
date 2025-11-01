use std::path::PathBuf;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

lazy_static! {
    static ref SETTINGS_PATH: PathBuf = {
        std::env::current_exe().unwrap().parent().unwrap().join("settings.json")
    };
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Settings {
    pub osc_mode: bool,
    pub osc_path: String,
    pub osc_range_start: f32,
    pub osc_range_end: f32,
    pub last_ble_mac: Option<String>,
}

impl Settings {
    pub fn save(&self) -> anyhow::Result<()> {
        let settings = serde_json::to_string_pretty(&self)?;
        std::fs::write((*SETTINGS_PATH).clone(), settings)?;
        Ok(())
    }

    pub fn load_or_default() -> anyhow::Result<Self> {
        if !(*SETTINGS_PATH).exists() {
            return Ok(Self::default());
        }

        let settings = std::fs::read_to_string((*SETTINGS_PATH).clone())?;
        let settings: Settings = serde_json::from_str(&settings)?;
        Ok(settings)
    }
}