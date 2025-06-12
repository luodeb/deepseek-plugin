use plugin_interfaces::{log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// 用户配置结构
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserConfig {
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            api_url: Some("https://api.deepseek.com/v1/chat/completions".to_string()),
        }
    }
}

/// 完整配置结构
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub plugin: toml::Value,
    pub user: Option<UserConfig>,
}

#[derive(Clone)]
pub struct ConfigManager {
    config_path: String,
}

impl ConfigManager {
    pub fn new(config_path: &str) -> Self {
        Self {
            config_path: config_path.to_string(),
        }
    }

    /// 从config.toml文件加载配置
    pub fn load_config(&self) -> Result<Config, Box<dyn std::error::Error>> {
        let config_path = Path::new(&self.config_path);

        if !config_path.exists() {
            return Err(format!("Config file not found: {}", config_path.display()).into());
        }

        let config_content = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&config_content)?;
        Ok(config)
    }

    /// 加载用户配置
    pub fn load_user_config(&self) -> UserConfig {
        match self.load_config() {
            Ok(config) => {
                if let Some(user_config) = config.user {
                    log_info!("Loaded user configuration from {}", self.config_path);
                    user_config
                } else {
                    log_info!("No user configuration found, using defaults");
                    UserConfig::default()
                }
            }
            Err(e) => {
                log_warn!("Failed to load user config: {}", e);
                UserConfig::default()
            }
        }
    }

    /// 保存用户配置到config.toml文件
    pub fn save_user_config(&self, api_key: &str, api_url: &str) {
        let config_path = Path::new(&self.config_path);

        // 读取现有配置
        let mut config = match self.load_config() {
            Ok(config) => config,
            Err(_) => {
                // 如果读取失败，创建默认配置
                Config {
                    plugin: toml::Value::Table(toml::map::Map::new()),
                    user: Some(UserConfig::default()),
                }
            }
        };

        // 更新用户配置
        let user_config = UserConfig {
            api_key: if api_key.trim().is_empty() {
                None
            } else {
                Some(api_key.to_string())
            },
            api_url: if api_url.trim().is_empty() {
                None
            } else {
                Some(api_url.to_string())
            },
        };

        config.user = Some(user_config);

        // 保存到文件
        match toml::to_string_pretty(&config) {
            Ok(toml_string) => {
                if let Err(e) = fs::write(config_path, toml_string) {
                    log_error!("Failed to save config to {}: {}", config_path.display(), e);
                } else {
                    log_info!(
                        "User configuration saved successfully to {}",
                        config_path.display()
                    );
                }
            }
            Err(e) => {
                log_error!("Failed to serialize config: {}", e);
            }
        }
    }
}
