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
    pub mode: ControlMode,
    pub osc_port: u16,
    pub osc_path: String,
    pub osc_range_start: f32,
    pub osc_range_end: f32,
    pub last_ble_mac: Option<String>,
    pub max_intensity_percent: u8,
    pub ngrok_token: Option<String>,
    pub remote_sync_local: bool,
}

impl Settings {
    pub fn save(&self) -> anyhow::Result<()> {
        let settings = serde_json::to_string_pretty(&self)?;
        std::fs::write((*SETTINGS_PATH).clone(), settings)?;
        Ok(())
    }

    pub fn load_or_default() -> anyhow::Result<Self> {
        if !(*SETTINGS_PATH).exists() {
            return Ok(Self {
                osc_port: 9001,
                osc_range_start: 0.0f32,
                osc_range_end: 1.0f32,
                max_intensity_percent: 100,
                ..Default::default()
            });
        }

        let settings = std::fs::read_to_string((*SETTINGS_PATH).clone())?;
        let settings: Settings = serde_json::from_str(&settings)?;
        Ok(settings)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum ControlMode {
    Manual,
    Osc,
    Remote(RemoteMode),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum RemoteMode {
    Sender,
    Receiver,
}

impl Default for ControlMode {
    fn default() -> Self {
        Self::Manual
    }
}