use serde::{Deserialize, Serialize};

/// DeepSeek API 响应中的选择项
#[derive(Deserialize, Debug)]
pub struct Choice {
    pub delta: Delta,
}

/// 消息增量（用于流式响应）
#[derive(Deserialize, Debug)]
pub struct Delta {
    pub content: Option<String>,
}

/// 流式响应数据块
#[derive(Deserialize, Debug)]
pub struct ChatCompletionChunk {
    pub choices: Vec<Choice>,
}

/// 消息结构
#[derive(Serialize, Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[allow(dead_code)]
impl Message {
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    pub fn user(content: &str) -> Self {
        Self::new("user", content)
    }

    pub fn assistant(content: &str) -> Self {
        Self::new("assistant", content)
    }

    pub fn system(content: &str) -> Self {
        Self::new("system", content)
    }
}
