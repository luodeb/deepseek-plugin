use futures_util::StreamExt;
use plugin_interfaces::{
    create_plugin_interface_from_handler, log_error, log_info, log_warn,
    pluginui::{Context, Ui},
    PluginHandler, PluginInterface, PluginMetadata, PluginStreamMessage,
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::{fs, path::Path};
use tokio::{runtime::Runtime, sync::Mutex};

/// DeepSeek API 响应中的选择项
#[derive(Deserialize, Debug)]
struct Choice {
    delta: Delta,
}

/// 消息增量（用于流式响应）
#[derive(Deserialize, Debug)]
struct Delta {
    content: Option<String>,
}

/// 流式响应数据块
#[derive(Deserialize, Debug)]
struct ChatCompletionChunk {
    choices: Vec<Choice>,
}

/// 消息结构
#[derive(Serialize, Clone)]
struct Message {
    role: String,
    content: String,
}

/// 用户配置结构
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UserConfig {
    api_key: Option<String>,
    api_url: Option<String>,
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
struct Config {
    plugin: toml::Value,
    user: Option<UserConfig>,
}

/// DeepSeek 对话插件
#[derive(Clone)]
pub struct DeepSeekPlugin {
    metadata: PluginMetadata,
    runtime: Option<Arc<Runtime>>,

    // 配置
    api_key: String,
    api_url: String,
    is_connected: bool,

    // HTTP 客户端
    client: Arc<Mutex<Option<reqwest::Client>>>,
}

impl DeepSeekPlugin {
    fn new() -> Self {
        Self {
            is_connected: false,
            metadata: PluginMetadata {
                id: "deepseek_plugin".to_string(),
                disabled: false,
                name: "DeepSeek Chat".to_string(),
                description: "DeepSeek AI 对话插件".to_string(),
                version: "1.0.0".to_string(),
                author: Some("Augment".to_string()),
                library_path: None,
                config_path: "config.toml".to_string(),
            },
            runtime: None,
            api_key: String::new(),
            api_url: "https://api.deepseek.com/v1/chat/completions".to_string(),
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// 更新配置并初始化客户端
    fn update_config(&self) {
        // 保存用户配置到文件
        self.save_user_config();

        // 初始化HTTP客户端
        if let Some(runtime) = &self.runtime {
            let client_arc = self.client.clone();
            runtime.spawn(async move {
                let client = reqwest::Client::new();
                let mut client_guard = client_arc.lock().await;
                *client_guard = Some(client);
                log_info!("HTTP client initialized");
            });
        }
    }

    /// 保存用户配置到config.toml文件
    fn save_user_config(&self) {
        let config_path = Path::new(&self.metadata.config_path);

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
            api_key: if self.api_key.trim().is_empty() {
                None
            } else {
                Some(self.api_key.clone())
            },
            api_url: if self.api_url.trim().is_empty() {
                None
            } else {
                Some(self.api_url.clone())
            },
        };

        config.user = Some(user_config);

        // 保存到文件
        match toml::to_string_pretty(&config) {
            Ok(toml_string) => {
                if let Err(e) = fs::write(config_path, toml_string) {
                    log_error!("Failed to save config to {}: {}", config_path.display(), e);
                } else {
                    log_info!("User configuration saved successfully to {}", config_path.display());
                }
            }
            Err(e) => {
                log_error!("Failed to serialize config: {}", e);
            }
        }
    }

    /// 从config.toml文件加载配置
    fn load_config(&self) -> Result<Config, Box<dyn std::error::Error>> {
        let config_path = Path::new(&self.metadata.config_path);

        if !config_path.exists() {
            return Err(format!("Config file not found: {}", config_path.display()).into());
        }

        let config_content = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&config_content)?;
        Ok(config)
    }

    /// 加载用户配置
    fn load_user_config(&mut self) {
        match self.load_config() {
            Ok(config) => {
                if let Some(user_config) = config.user {
                    if let Some(api_key) = user_config.api_key {
                        self.api_key = api_key;
                        log_info!("Loaded API key from config");
                    }
                    if let Some(api_url) = user_config.api_url {
                        self.api_url = api_url;
                        log_info!("Loaded API URL from config");
                    }
                } else {
                    log_info!("No user configuration found, using defaults");
                }
            }
            Err(e) => {
                log_warn!("Failed to load user config: {}", e);
            }
        }
    }

    /// 发送流式请求到 DeepSeek API
    async fn send_streaming_request(
        self: Arc<Self>,
        message: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.api_key.trim().is_empty() {
            return Err("API Key 未设置".into());
        }

        // 获取客户端
        let client = {
            let client_guard = self.client.lock().await;
            client_guard.clone()
        };

        let client = client.ok_or("HTTP 客户端未初始化")?;

        // 构建请求头
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))?,
        );

        // 构建请求体
        let messages = vec![Message {
            role: "user".to_string(),
            content: message,
        }];

        let request_body = json!({
            "model": "deepseek-chat",
            "messages": messages,
            "stream": true
        });

        log_info!("Sending streaming request to DeepSeek API");

        // 发送请求
        let response = client
            .post(&self.api_url)
            .headers(headers)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("API 请求失败: {}", error_text).into());
        }

        // 开始流式传输
        let stream_id = match self.send_message_stream_start("chat", Some("DeepSeek 对话")) {
            Ok(id) => id,
            Err(e) => return Err(format!("启动流式传输失败: {}", e).into()),
        };

        let mut stream = response.bytes_stream();
        let mut has_content = false;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let chunk_str = String::from_utf8_lossy(&chunk);

            // 处理 SSE 格式的数据
            for line in chunk_str.split("\n\n") {
                if line.starts_with("data: ") {
                    let data = &line[6..];

                    // 检查是否为结束标记
                    if data == "[DONE]" {
                        log_info!("Stream completed");
                        let _ = self.send_message_stream_end(&stream_id, true, None);
                        return Ok(());
                    }

                    // 解析 JSON
                    match serde_json::from_str::<ChatCompletionChunk>(data) {
                        Ok(chunk_data) => {
                            for choice in chunk_data.choices {
                                if let Some(content) = choice.delta.content {
                                    has_content = true;
                                    if let Err(e) =
                                        self.send_message_stream(&stream_id, &content, false)
                                    {
                                        log_error!("Failed to send stream chunk: {}", e);
                                        let _ = self.send_message_stream_end(
                                            &stream_id,
                                            false,
                                            Some(&e.to_string()),
                                        );
                                        return Err(e.into());
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log_warn!("Failed to parse chunk: {} - Data: {}", e, data);
                        }
                    }
                }
            }
        }

        if has_content {
            let _ = self.send_message_stream_end(&stream_id, true, None);
        } else {
            let _ = self.send_message_stream_end(&stream_id, false, Some("未收到有效回复"));
        }

        Ok(())
    }
}

impl PluginHandler for DeepSeekPlugin {
    fn update_ui(&mut self, _ctx: &Context, ui: &mut Ui) {
        ui.label("DeepSeek AI 配置");

        // API Key 输入
        ui.horizontal(|ui| {
            ui.label("API Key:");
            let api_key_response = ui.text_edit_singleline(&mut self.api_key);
            if api_key_response.changed() {
                log_info!("API Key updated");
                self.update_config();
            }
        });

        // API URL 输入
        ui.horizontal(|ui| {
            ui.label("API URL:");
            let url_response = ui.text_edit_singleline(&mut self.api_url);
            if url_response.changed() {
                log_info!("API URL updated");
                self.update_config();
            }
        });

        // 状态显示
        if self.api_key.trim().is_empty() {
            ui.label("状态: 请设置 API Key");
        } else {
            ui.label("状态: 已配置，可以开始对话");
        }
    }

    fn on_mount(&mut self, metadata: &PluginMetadata) -> Result<(), Box<dyn std::error::Error>> {
        log_info!("[{}] Plugin mount successfully", self.metadata.name);
        self.metadata = metadata.clone();

        // 加载用户配置
        self.load_user_config();

        // 初始化 tokio 异步运行时
        match Runtime::new() {
            Ok(runtime) => {
                self.runtime = Some(Arc::new(runtime));
                log_info!("Tokio runtime initialized successfully");
                self.update_config();
            }
            Err(e) => {
                log_warn!("Failed to initialize tokio runtime: {}", e);
            }
        }

        Ok(())
    }

    fn on_dispose(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log_info!("[{}] Plugin disposed successfully", self.metadata.name);
        Ok(())
    }

    fn on_connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log_info!("[{}] Connected", self.metadata.name);
        self.is_connected = true;
        Ok(())
    }

    fn on_disconnect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log_warn!("[{}] Disconnected", self.metadata.name);
        self.is_connected = false;
        Ok(())
    }

    fn handle_message(&self, message: &str) -> Result<String, Box<dyn std::error::Error>> {
        if !self.is_connected {
            return Err("插件未连接".into());
        }
        log_info!("[{}] Received message: {}", self.metadata.name, message);

        if self.api_key.trim().is_empty() {
            return Err("请先在插件配置中设置 API Key".into());
        }

        // 启动异步任务处理流式请求
        if let Some(runtime) = &self.runtime {
            let self_arc = Arc::new(self.clone());
            let message_clone = message.to_string();

            runtime.spawn(async move {
                if let Err(e) = self_arc.send_streaming_request(message_clone).await {
                    log_error!("Failed to send streaming request: {}", e);
                }
            });

            Ok("正在处理您的请求...".to_string())
        } else {
            Err("运行时未初始化".into())
        }
    }

    fn get_metadata(&self) -> PluginMetadata {
        self.metadata.clone()
    }
}

/// 创建插件实例的导出函数
#[no_mangle]
pub extern "C" fn create_plugin() -> *mut PluginInterface {
    let plugin = DeepSeekPlugin::new();
    let handler: Box<dyn PluginHandler> = Box::new(plugin);
    create_plugin_interface_from_handler(handler)
}

/// 销毁插件实例的导出函数
#[no_mangle]
pub extern "C" fn destroy_plugin(interface: *mut PluginInterface) {
    if !interface.is_null() {
        unsafe {
            ((*interface).destroy)((*interface).plugin_ptr);
            let _ = Box::from_raw(interface);
        }
    }
}
