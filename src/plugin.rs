use plugin_interfaces::{
    log_error, log_info, log_warn,
    pluginui::{Context, Ui},
    PluginHandler, PluginInstanceContext, StreamError,
};
use std::sync::Arc;
use tokio::runtime::Runtime;

use crate::api::{ApiClient, Message};
use crate::config::ConfigManager;
use crate::history::HistoryProcessor;

/// DeepSeek 对话插件
#[derive(Clone)]
pub struct DeepSeekPlugin {
    runtime: Option<Arc<Runtime>>,

    // 配置
    api_key: String,
    api_url: String,

    // 组件
    api_client: Option<ApiClient>,
    config_manager: ConfigManager,
}

impl DeepSeekPlugin {
    pub fn new() -> Self {
        Self {
            runtime: None,
            api_key: String::new(),
            api_url: "https://api.deepseek.com/v1/chat/completions".to_string(),
            api_client: None,
            config_manager: ConfigManager::new("user.toml"),
        }
    }

    /// 更新配置并初始化客户端
    fn update_config(&mut self) {
        // 保存用户配置到文件
        self.config_manager
            .save_user_config(&self.api_key, &self.api_url);

        // 初始化API客户端
        self.api_client = Some(ApiClient::new(self.api_key.clone(), self.api_url.clone()));

        // 初始化HTTP客户端
        if let (Some(runtime), Some(api_client)) = (&self.runtime, &self.api_client) {
            let client = api_client.clone();
            runtime.spawn(async move {
                client.initialize().await;
            });
        }
    }

    /// 加载用户配置
    fn load_user_config(&mut self) {
        let user_config = self.config_manager.load_user_config();

        if let Some(api_key) = user_config.api_key {
            self.api_key = api_key;
            log_info!("Loaded API key from config");
        }
        if let Some(api_url) = user_config.api_url {
            self.api_url = api_url;
            log_info!("Loaded API URL from config");
        }
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

        let api_client = self.api_client.as_ref().ok_or("API 客户端未初始化")?;

        // 构建消息列表
        let mut messages = Vec::new();

        // 处理历史消息
        if let Some(history_vec) = plugin_ctx.get_history() {
            let historical_messages =
                HistoryProcessor::extract_completed_messages(history_vec.clone());
            messages.extend(historical_messages);
            log_info!("Loaded {} completed historical messages", messages.len());
        } else {
            log_info!("No history available");
        }

        // 添加当前用户消息
        messages.push(Message::user(&message));

        log_info!(
            "Sending {} total messages to AI (including current message)",
            messages.len()
        );

        // 发送请求
        let self_clone1 = self.clone();
        let self_clone2 = self.clone();
        let self_clone3 = self.clone();

        api_client
            .send_streaming_request(
                messages,
                plugin_ctx,
                move |ctx| self_clone1.send_message_stream_start(ctx),
                move |stream_id, content, is_final, ctx| {
                    self_clone2.send_message_stream(stream_id, content, is_final, ctx)
                },
                move |stream_id, success, error_msg, ctx| {
                    self_clone3.send_message_stream_end(stream_id, success, error_msg, ctx)
                },
            )
            .await
    }

    /// 开始流式消息传输
    fn send_message_stream_start(
        &self,
        _plugin_ctx: &PluginInstanceContext,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // 简化实现，返回一个固定的流ID
        Ok("stream_001".to_string())
    }

    /// 发送流式消息块
    fn send_message_stream(
        &self,
        _stream_id: &str,
        _content: &str,
        _is_final: bool,
        _plugin_ctx: &PluginInstanceContext,
    ) -> Result<(), StreamError> {
        // 简化实现，暂时不做实际的流式传输
        Ok(())
    }

    /// 结束流式消息传输
    fn send_message_stream_end(
        &self,
        _stream_id: &str,
        _success: bool,
        _error_msg: Option<&str>,
        _plugin_ctx: &PluginInstanceContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 简化实现，暂时不做实际的流式传输结束
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
            "Plugin Receive Message. Metadata: id={}, name={}, version={}, instance_id={}",
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
            let context_clone = plugin_ctx.clone();

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
