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
    /// Time-To-Live in seconds for cached media before being eligible for LRU pruning (default: 7 days)
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: Option<u64>,
    /// Maximum allowed file upload/stream size in bytes (default: 100MB)
    #[serde(default = "default_max_file_size")]
    pub max_file_size: Option<u64>,
}

fn default_cache_ttl() -> Option<u64> {
    Some(604_800) // 7 days
}

fn default_max_file_size() -> Option<u64> {
    Some(100 * 1024 * 1024) // 100 MB
}

impl Media {
    pub fn validate(&self) -> Result<(), String> {
        if self.dir.trim().is_empty() {
            return Err("The 'dir' in [media] cannot be empty.".to_string());
        }
        if let Some(0) = self.cache_ttl_seconds {
            return Err("The 'cache_ttl_seconds' in [media] must be greater than 0.".to_string());
        }
        if let Some(0) = self.max_file_size {
            return Err("The 'max_file_size' in [media] must be greater than 0.".to_string());
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

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Webhook {
    /// Target HTTP URL for webhook event delivery
    pub url: Option<String>,
    /// Shared secret key used for HMAC-SHA256 signature in `X-Uwah-Signature` header
    pub secret: Option<String>,
    /// Global toggle enabling or disabling webhook dispatch
    #[serde(default)]
    pub enabled: bool,
}

impl Webhook {
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled {
            if let Some(ref url) = self.url {
                if url.trim().is_empty() {
                    return Err(
                        "The 'url' in [webhook] cannot be empty when enabled is true.".to_string(),
                    );
                }
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err(
                        "The 'url' in [webhook] must start with http:// or https://.".to_string(),
                    );
                }
            } else {
                return Err(
                    "The 'url' in [webhook] must be configured when enabled is true.".to_string(),
                );
            }
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
    #[serde(default)]
    pub webhook: Option<Webhook>,
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
        if let Some(ref webhook) = self.webhook {
            webhook
                .validate()
                .map_err(|err| format!("Invalid webhook config: {}", err))?;
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
