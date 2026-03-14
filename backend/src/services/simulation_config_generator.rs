use crate::services::llm_client::{LLMClient, ChatMessage};
use crate::services::zep_entity_reader::EntityNode;
use serde_json::Value;

pub struct SimulationConfigGenerator {
    llm: LLMClient,
}

impl SimulationConfigGenerator {
    pub fn new(llm: LLMClient) -> Self {
        Self { llm }
    }

    pub async fn generate_config(
        &self,
        simulation_requirement: &str,
        document_text: &str,
        entities: &[EntityNode],
        max_rounds: usize,
        _enable_twitter: bool,
        _enable_reddit: bool,
    ) -> anyhow::Result<Value> {
        let entity_summary: Vec<String> = entities.iter()
            .map(|e| format!("- {} ({})", e.name, e.get_entity_type().unwrap_or_default()))
            .collect();

        let doc_preview = if document_text.len() > 5000 {
            &document_text[..5000]
        } else {
            document_text
        };

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are an expert simulation configuration designer. Output valid JSON only.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    r#"Design simulation parameters for a social media simulation.

Requirement: {}

Entities:
{}

Document preview:
{}

Output JSON with:
{{
    "max_rounds": {},
    "simulation_topic": "main topic",
    "initial_posts": ["seed post 1", "seed post 2"],
    "agent_configs": [
        {{
            "entity_name": "name",
            "activity_level": "high|medium|low",
            "sentiment": "positive|neutral|negative",
            "influence_weight": 0.0-1.0,
            "posting_frequency": 0.0-1.0
        }}
    ],
    "generation_reasoning": "explanation of config choices"
}}"#,
                    simulation_requirement,
                    entity_summary.join("\n"),
                    doc_preview,
                    max_rounds
                ),
            },
        ];

        let result = self.llm.chat_json(&messages, 0.5, 4096).await?;
        Ok(result)
    }
}
