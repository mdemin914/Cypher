use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct AppSettings {
    pub host_name: Option<String>,
    pub midi_port_name: Option<String>,
    pub midi_channel: u8,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_size: Option<u32>,
    pub input_latency_compensation_ms: f32,
    pub last_sampler_kit: Option<PathBuf>,
    pub last_synth_preset: Option<PathBuf>,
    pub last_theme: Option<PathBuf>,
    pub bpm_rounding: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            host_name: None,
            midi_port_name: None,
            midi_channel: 0,
            input_device: None,
            output_device: None,
            sample_rate: None,
            buffer_size: None,
            input_latency_compensation_ms: 5.0, // Default to 5ms safety buffer
            last_sampler_kit: None,
            last_synth_preset: None,
            last_theme: None,
            bpm_rounding: false,
        }
    }
}

pub fn get_config_dir() -> Option<PathBuf> {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let app_settings_dir = exe_dir.join("AppSettings");

            // Ensure main directory and subdirectories exist
            for dir in [
                &app_settings_dir,
                &app_settings_dir.join("Samples"),
                &app_settings_dir.join("SynthPresets"),
                &app_settings_dir.join("Kits"),
                &app_settings_dir.join("Themes"),
                &app_settings_dir.join("LiveRecordings"),
                &app_settings_dir.join("Sessions"), // Added this line
            ] {
                if !dir.exists() {
                    if let Err(e) = fs::create_dir_all(dir) {
                        eprintln!(
                            "Failed to create directory at {}: {}",
                            dir.display(),
                            e
                        );
                        return None;
                    }
                }
            }
            return Some(app_settings_dir);
        }
    }
    eprintln!("Could not determine application directory.");
    None
}

pub fn save_settings(settings: &AppSettings) {
    if let Some(dir) = get_config_dir() {
        let path = dir.join("settings.json");
        match serde_json::to_string_pretty(settings) {
            Ok(json_string) => {
                if let Err(e) = fs::write(&path, json_string) {
                    eprintln!("Failed to write settings to {}: {}", path.display(), e);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize settings: {}", e);
            }
        }
    }
}

pub fn load_settings() -> AppSettings {
    if let Some(dir) = get_config_dir() {
        let path = dir.join("settings.json");
        if path.exists() {
            return match fs::read_to_string(&path) {
                Ok(json_string) => match serde_json::from_str(&json_string) {
                    Ok(settings) => settings,
                    Err(e) => {
                        eprintln!("Failed to parse settings file, using defaults. Error: {}", e);
                        AppSettings::default()
                    }
                },
                Err(e) => {
                    eprintln!("Failed to read settings file, using defaults. Error: {}", e);
                    AppSettings::default()
                }
            };
        }
    }
    AppSettings::default()
}