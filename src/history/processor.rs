use crate::api::types::Message;
use plugin_interfaces::{log_info, log_warn, HistoryMessage};

pub struct HistoryProcessor;

impl HistoryProcessor {
    /// 从历史记录中提取已完成的消息
    pub fn extract_completed_messages(history: Vec<HistoryMessage>) -> Vec<Message> {
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

    /// 从历史记录中提取最近的N条已完成消息
    #[allow(dead_code)]
    pub fn extract_recent_completed_messages(
        history: Vec<HistoryMessage>,
        limit: usize,
    ) -> Vec<Message> {
        let mut completed_messages: Vec<&HistoryMessage> = history
            .iter()
            .filter(|msg| msg.status == "completed")
            .collect();

        // 按创建时间排序（最新的在前）
        completed_messages.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // 取最近的N条消息
        let recent_messages: Vec<&HistoryMessage> =
            completed_messages.into_iter().take(limit).collect();

        // 重新按时间顺序排列（最旧的在前）
        let mut recent_messages = recent_messages;
        recent_messages.reverse();

        log_info!(
            "Extracted {} recent completed messages from {} total history messages",
            recent_messages.len(),
            history.len()
        );

        // 转换为 AI 消息格式
        let mut messages = Vec::new();
        for history_msg in recent_messages {
            if !history_msg.content.trim().is_empty() {
                let ai_role = match history_msg.role.as_str() {
                    "user" => "user",
                    "plugin" => "assistant",
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
            }
        }

        messages
    }
}
