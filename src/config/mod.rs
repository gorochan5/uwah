use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::OnceLock;
use tracing::{error, info};


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Database {
    pub dir: String,
}

impl Database {
    pub fn validate(&self) -> Result<(), String> {
        if self.dir.trim().is_empty() {
            return Err("The 'dir' in [database] cannot be empty.".to_string());
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Service {
    pub host: String,
    pub port: u16,
    pub auth: String,
}

impl Service {
    pub fn validate(&self) -> Result<(), String> {
        if self.host.trim().is_empty() {
            return Err("The 'host' in [service] cannot be empty.".to_string());
        }
        if self.auth.trim().is_empty() {
            return Err("The 'auth' in [service] cannot be empty.".to_string());
        }
        if self.port == 0 {
            return Err("The 'port' in [service] must be a valid port number.".to_string());
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Media {
    pub dir: String,
}

impl Media {
    pub fn validate(&self) -> Result<(), String> {
        if self.dir.trim().is_empty() {
            return Err("The 'dir' in [media] cannot be empty.".to_string());
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Sessions {
    /// Directory where whatsapp-rust per-session SQLite stores are created.
    /// Each active WhatsApp account gets its own `<sessions_dir>/<id>.db` file.
    pub dir: String,
}

impl Sessions {
    pub fn validate(&self) -> Result<(), String> {
        if self.dir.trim().is_empty() {
            return Err("The 'dir' in [sessions] cannot be empty.".to_string());
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub database: Database,
    pub service: Service,
    pub media: Media,
    pub sessions: Sessions,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if let Err(err) = self.database.validate() {
            return Err(format!("Invalid database config: {}", err));
        }
        if let Err(err) = self.service.validate() {
            return Err(format!("Invalid service config: {}", err));
        }
        if let Err(err) = self.media.validate() {
            return Err(format!("Invalid media config: {}", err));
        }
        if let Err(err) = self.sessions.validate() {
            return Err(format!("Invalid sessions config: {}", err));
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

pub fn init() -> anyhow::Result<()> {
    let path = std::path::Path::new("uwah.toml");

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