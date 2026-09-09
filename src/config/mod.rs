use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;
use tracing::{error, info};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub token: String,
    pub media_dir: String,
    pub db_dir: String,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.token.trim().is_empty() {
            return Err(format!("The 'token' in [config.{}] cannot be empty.", path));
        }
        if self.media_dir.trim().is_empty() {
            return Err(format!("The 'media_dir' in [config.{}] cannot be empty.", path));
        }
        if self.db_dir.trim().is_empty() {
            return Err(format!("The 'db_dir' in [config.{}] cannot be empty.", path));
        }
        Ok(())
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn get() -> &'static Config {
    CONFIG
        .get()
        .expect("config::init() must be called before config::get()")
}

pub fn try_get() -> Option<&'static Config> {
    CONFIG.get()
}

pub fn init() -> anyhow::Result<()> {
    let path = std::path::Path::new("kronos_pdv.toml");

    if !path.is_file() {
        anyhow::bail!(
            "Config file not found in '{}'. Create the file with the credentials before starting.",
            path.display()
        );
    }

    info!("Config file found in {}", path.display());

    let config_file = fs::read_to_string(path)
        .with_context(|| format!("Error reading config file in {}", path.display()))?;

    let config: Config = toml::from_str(&config_file).with_context(|| {
        format!(
            "Syntax error or missing required field in TOML ({})",
            path.display()
        )
    })?;

    if let Err(err_msg) = config.validate() {
        error!("Invalid config in TOML file: {}", err_msg);
        anyhow::bail!(
            "Invalid config in TOML file ({}): {}",
            path.display(),
            err_msg
        );
    }

    CONFIG
        .set(config)
        .map_err(|_| anyhow::anyhow!("config::init() was called more than once"))?;

    Ok(())
}