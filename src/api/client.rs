use futures_util::StreamExt;
use plugin_interfaces::{log_info, log_warn, PluginInstanceContext, StreamError};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::types::{ChatCompletionChunk, Message};

#[derive(Clone)]
pub struct ApiClient {
    client: Arc<Mutex<Option<reqwest::Client>>>,
    api_key: String,
    api_url: String,
}

impl ApiClient {
    pub fn new(api_key: String, api_url: String) -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
            api_key,
            api_url,
        }
    }

    pub async fn initialize(&self) {
        let client = reqwest::Client::new();
        let mut client_guard = self.client.lock().await;
        *client_guard = Some(client);
        log_info!("HTTP client initialized");
    }

    pub async fn send_streaming_request<F1, F2, F3>(
        &self,
        messages: Vec<Message>,
        plugin_ctx: &PluginInstanceContext,
        send_message_stream_start: F1,
        send_message_stream: F2,
        send_message_stream_end: F3,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F1: Fn(&PluginInstanceContext) -> Result<String, Box<dyn std::error::Error>>,
        F2: Fn(&str, &str, bool, &PluginInstanceContext) -> Result<(), StreamError>,
        F3: Fn(
            &str,
            bool,
            Option<&str>,
            &PluginInstanceContext,
        ) -> Result<(), Box<dyn std::error::Error>>,
    {
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
        let request_body = json!({
            "model": "deepseek-chat",
            "messages": messages,
            "stream": true
        });

        log_info!(
            "Sending streaming request to DeepSeek API with {} messages",
            messages.len()
        );

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
        let stream_id = match send_message_stream_start(plugin_ctx) {
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
                        let _ = send_message_stream_end(&stream_id, true, None, plugin_ctx);
                        return Ok(());
                    }

                    // 解析 JSON
                    match serde_json::from_str::<ChatCompletionChunk>(data) {
                        Ok(chunk_data) => {
                            for choice in chunk_data.choices {
                                if let Some(content) = choice.delta.content {
                                    has_content = true;
                                    if let Err(e) =
                                        send_message_stream(&stream_id, &content, false, plugin_ctx)
                                    {
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
                                                let _ = send_message_stream_end(
                                                    &stream_id,
                                                    false,
                                                    Some(&format!("Error: {}", e)),
                                                    &plugin_ctx,
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
            let _ = send_message_stream_end(&stream_id, true, None, plugin_ctx);
        } else {
            let _ = send_message_stream_end(&stream_id, false, Some("未收到有效回复"), plugin_ctx);
        }

        Ok(())
    }
}
