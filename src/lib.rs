use futures_util::StreamExt;
use plugin_interfaces::{
    create_plugin_interface_from_handler, log_error, log_info, log_warn,
    pluginui::{Context, Ui},
    HistoryMessage, PluginHandler, PluginInstanceContext, PluginInterface, PluginStreamMessage,
    StreamError,
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

impl Message {
    fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
        }
    }
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
    runtime: Option<Arc<Runtime>>,

    // 配置
    api_key: String,
    api_url: String,

    // HTTP 客户端
    client: Arc<Mutex<Option<reqwest::Client>>>,
}

impl DeepSeekPlugin {
    fn new() -> Self {
        Self {
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
        let config_path = Path::new("user.toml");

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

    /// 从config.toml文件加载配置
    fn load_config(&self) -> Result<Config, Box<dyn std::error::Error>> {
        let config_path = Path::new("user.toml");

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

    /// 从历史记录中提取已完成的消息
    fn extract_completed_messages(&self, history: Vec<HistoryMessage>) -> Vec<Message> {
        let mut messages = Vec::new();

        // 过滤状态为 completed 的消息
        let completed_messages: Vec<&HistoryMessage> = history
            .iter()
            .filter(|msg| msg.status == "completed")
            .collect();

        log_info!(
            "Found {} completed messages out of {} total history messages",
            completed_messages.len(),
            history.len()
        );

        // 转换为 AI 消息格式
        for history_msg in completed_messages {
            if !history_msg.content.trim().is_empty() {
                // 根据角色转换消息
                let ai_role = match history_msg.role.as_str() {
                    "user" => "user",
                    "plugin" => "assistant", // 插件回复作为助手回复
                    "system" => "system",
                    _ => {
                        log_warn!(
                            "Unknown role '{}' in history message, treating as user",
                            history_msg.role
                        );
                        "user"
                    }
                };

                messages.push(Message::new(ai_role, &history_msg.content));

                log_info!(
                    "Added message: role={}, content_length={}",
                    ai_role,
                    history_msg.content.len()
                );
            }
        }

        messages
    }

    /// 发送流式请求到 DeepSeek API
    async fn send_streaming_request(
        self: Arc<Self>,
        message: String,
        plugin_ctx: &PluginInstanceContext,
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
        let history = plugin_ctx.get_history();
        let messages = if let Some(history_vec) = history {
            self.extract_completed_messages(history_vec.clone())
        } else {
            Vec::new()
        };
        let mut messages = messages;
        messages.push(Message::new("user", &message));

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
        let stream_id = match self.send_message_stream_start(plugin_ctx) {
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
                if let Some(data) = line.strip_prefix("data: ") {
                    // 检查是否为结束标记
                    if data == "[DONE]" {
                        log_info!("Stream completed");
                        let _ = self.send_message_stream_end(&stream_id, true, None, plugin_ctx);
                        return Ok(());
                    }

                    // 解析 JSON
                    match serde_json::from_str::<ChatCompletionChunk>(data) {
                        Ok(chunk_data) => {
                            for choice in chunk_data.choices {
                                if let Some(content) = choice.delta.content {
                                    has_content = true;
                                    if let Err(e) = self.send_message_stream(
                                        &stream_id, &content, false, plugin_ctx,
                                    ) {
                                        match e {
                                            StreamError::StreamCancelled => {
                                                log_info!(
                                        "Stream {} was cancelled by user, stopping gracefully...",
                                        stream_id
                                    );
                                                return Ok(()); // 用户取消，直接返回，不发送错误消息
                                            }
                                            _ => {
                                                log_warn!(
                                                    "Failed to send background stream chunk: {}",
                                                    e
                                                );
                                                let _ = self.send_message_stream_end(
                                                    &stream_id,
                                                    false,
                                                    Some(&format!("Error: {}", e)),
                                                    plugin_ctx,
                                                );
                                                return Err(e.into());
                                            }
                                        }
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
            let _ = self.send_message_stream_end(&stream_id, true, None, plugin_ctx);
        } else {
            let _ =
                self.send_message_stream_end(&stream_id, false, Some("未收到有效回复"), plugin_ctx);
        }

        Ok(())
    }
}

impl PluginHandler for DeepSeekPlugin {
    fn update_ui(&mut self, _ctx: &Context, ui: &mut Ui, _plugin_ctx: &PluginInstanceContext) {
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
        if self.api_key.trim().is_empty() || self.api_url.trim().is_empty() {
            ui.label("状态: 请设置 API Key 和 URL");
        } else {
            ui.label("状态: 已配置，可以开始对话");
        }
    }

    fn on_mount(
        &mut self,
        plugin_ctx: &PluginInstanceContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = plugin_ctx.get_metadata();
        log_info!("[{}] Plugin mount successfully", metadata.name);
        log_info!(
            "Config Metadata: id={}, name={}, version={}, instance_id={}",
            metadata.id,
            metadata.name,
            metadata.version,
            metadata.instance_id.clone().unwrap_or("None".to_string())
        );

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

    fn on_dispose(
        &mut self,
        plugin_ctx: &PluginInstanceContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = plugin_ctx.get_metadata();
        log_info!(
            "Plugin disposed successfully. Metadata: id={}, name={}, version={}, instance_id={}",
            metadata.id,
            metadata.name,
            metadata.version,
            metadata.instance_id.clone().unwrap_or("None".to_string())
        );
        // 关闭 tokio 异步运行时
        if let Some(runtime) = self.runtime.clone() {
            // Use Arc::try_unwrap to get ownership if this is the last reference
            match Arc::try_unwrap(runtime) {
                Ok(runtime) => {
                    runtime.shutdown_timeout(std::time::Duration::from_millis(10));
                    log_info!("Tokio runtime shutdown successfully");
                }
                Err(_) => {
                    log_warn!("Cannot shutdown runtime: other references still exist");
                }
            }
        } else {
            log_warn!("Tokio runtime not initialized, cannot shutdown");
        }
        Ok(())
    }

    fn on_connect(
        &mut self,
        plugin_ctx: &PluginInstanceContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = plugin_ctx.get_metadata();
        log_info!(
            "Plugin connect successfully. Metadata: id={}, name={}, version={}, instance_id={}",
            metadata.id,
            metadata.name,
            metadata.version,
            metadata.instance_id.clone().unwrap_or("None".to_string())
        );

        // 校验是否配置了 api 和 key
        if self.api_key.trim().is_empty() || self.api_url.trim().is_empty() {
            log_warn!("API Key not configured, please set in plugin settings");
            return Err("API Key not configured".into());
        }

        Ok(())
    }

    fn on_disconnect(
        &mut self,
        plugin_ctx: &PluginInstanceContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = plugin_ctx.get_metadata();
        log_info!(
            "Plugin disconnect successfully. Metadata: id={}, name={}, version={}, instance_id={}",
            metadata.id,
            metadata.name,
            metadata.version,
            metadata.instance_id.clone().unwrap_or("None".to_string())
        );
        Ok(())
    }

    fn handle_message(
        &mut self,
        message: &str,
        plugin_ctx: &PluginInstanceContext,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let metadata = plugin_ctx.get_metadata();
        log_info!(
            "Plugin Recive Message. Metadata: id={}, name={}, version={}, instance_id={}",
            metadata.id,
            metadata.name,
            metadata.version,
            metadata.instance_id.clone().unwrap_or("None".to_string())
        );

        if self.api_key.trim().is_empty() {
            return Err("请先在插件配置中设置 API Key".into());
        }

        // 启动异步任务处理流式请求
        if let Some(runtime) = &self.runtime {
            let self_arc = Arc::new(self.clone());
            let message_clone = message.to_string();
            let context_clone = plugin_ctx.clone(); // 克隆上下文以便在异步任务中使用

            runtime.spawn(async move {
                if let Err(e) = self_arc
                    .send_streaming_request(message_clone, &context_clone)
                    .await
                {
                    log_error!("Failed to send streaming request: {}", e);
                }
            });

            Ok("正在处理您的请求...".to_string())
        } else {
            Err("运行时未初始化".into())
        }
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
///
/// # Safety
///
/// This function is unsafe because it dereferences raw pointers.
/// The caller must ensure that:
/// - `interface` is a valid pointer to a `PluginInterface` that was created by `create_plugin`
/// - `interface` has not been freed or destroyed previously
/// - The `PluginInterface` and its associated plugin instance are in a valid state
#[no_mangle]
pub unsafe extern "C" fn destroy_plugin(interface: *mut PluginInterface) {
    if !interface.is_null() {
        ((*interface).destroy)((*interface).plugin_ptr);
        let _ = Box::from_raw(interface);
    }
}
