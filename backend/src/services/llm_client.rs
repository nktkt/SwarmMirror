use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use regex::Regex;

#[derive(Clone)]
pub struct LLMClient {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

impl LLMClient {
    pub fn new(api_key: &str, base_url: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: f64,
        max_tokens: u32,
        response_format: Option<Value>,
    ) -> anyhow::Result<String> {
        let url = format!("{}/chat/completions", self.base_url);

        let request = ChatRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            temperature,
            max_tokens,
            response_format,
        };

        let resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            anyhow::bail!("LLM API error ({}): {}", status, body);
        }

        let chat_resp: ChatResponse = serde_json::from_str(&body)?;
        let content = chat_resp.choices.first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        // Remove <think> tags from reasoning models
        let re = Regex::new(r"<think>[\s\S]*?</think>").unwrap();
        let cleaned = re.replace_all(&content, "").trim().to_string();

        Ok(cleaned)
    }

    pub async fn chat_json(
        &self,
        messages: &[ChatMessage],
        temperature: f64,
        max_tokens: u32,
    ) -> anyhow::Result<Value> {
        let response = self.chat(
            messages, temperature, max_tokens,
            Some(json!({"type": "json_object"})),
        ).await?;

        // Clean markdown code fences
        let cleaned = response.trim();
        let re_start = Regex::new(r"^```(?:json)?\s*\n?").unwrap();
        let re_end = Regex::new(r"\n?```\s*$").unwrap();
        let cleaned = re_start.replace(cleaned, "");
        let cleaned = re_end.replace(&cleaned, "");
        let cleaned = cleaned.trim();

        let parsed: Value = serde_json::from_str(cleaned)
            .map_err(|e| anyhow::anyhow!("Invalid JSON from LLM: {}: {}", e, cleaned))?;

        Ok(parsed)
    }
}
