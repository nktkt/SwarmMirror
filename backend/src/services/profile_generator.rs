use crate::services::llm_client::{LLMClient, ChatMessage};
use crate::services::zep_entity_reader::EntityNode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub user_id: usize,
    pub user_name: String,
    pub name: String,
    pub bio: String,
    pub persona: String,
    pub karma: i32,
    pub friend_count: i32,
    pub follower_count: i32,
    pub statuses_count: i32,
    pub age: Option<i32>,
    pub gender: Option<String>,
    pub mbti: Option<String>,
    pub profession: Option<String>,
    pub interested_topics: Vec<String>,
    pub source_entity_uuid: Option<String>,
    pub source_entity_type: Option<String>,
}

pub struct ProfileGenerator {
    llm: LLMClient,
}

impl ProfileGenerator {
    pub fn new(llm: LLMClient) -> Self {
        Self { llm }
    }

    pub async fn generate_profiles(
        &self, entities: &[EntityNode], use_llm: bool,
    ) -> anyhow::Result<Vec<AgentProfile>> {
        let mut profiles = Vec::new();

        for (i, entity) in entities.iter().enumerate() {
            let profile = if use_llm {
                self.generate_with_llm(i, entity).await
                    .unwrap_or_else(|_| self.generate_basic(i, entity))
            } else {
                self.generate_basic(i, entity)
            };
            profiles.push(profile);
        }
        Ok(profiles)
    }

    fn generate_basic(&self, index: usize, entity: &EntityNode) -> AgentProfile {
        let entity_type = entity.get_entity_type().unwrap_or_else(|| "Person".to_string());
        AgentProfile {
            user_id: index + 1,
            user_name: entity.name.to_lowercase().replace(' ', "_"),
            name: entity.name.clone(),
            bio: entity.summary.clone(),
            persona: format!("I am {}. {}", entity.name, entity.summary),
            karma: 1000,
            friend_count: 100,
            follower_count: 150,
            statuses_count: 500,
            age: None,
            gender: None,
            mbti: None,
            profession: Some(entity_type.clone()),
            interested_topics: vec![],
            source_entity_uuid: Some(entity.uuid.clone()),
            source_entity_type: Some(entity_type),
        }
    }

    async fn generate_with_llm(&self, index: usize, entity: &EntityNode) -> anyhow::Result<AgentProfile> {
        let entity_type = entity.get_entity_type().unwrap_or_else(|| "Person".to_string());

        let context = format!(
            "Entity: {}\nType: {}\nSummary: {}\nRelated facts: {}",
            entity.name, entity_type, entity.summary,
            entity.related_edges.iter()
                .filter_map(|e| e["fact"].as_str())
                .collect::<Vec<_>>().join("; ")
        );

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are an expert at creating detailed social media user profiles. Output valid JSON only.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    r#"Create a detailed social media profile for this entity. Output JSON:
{{
    "name": "display name",
    "bio": "short bio (1-2 sentences)",
    "persona": "detailed personality description (3-5 sentences)",
    "age": number or null,
    "gender": "string or null",
    "mbti": "MBTI type or null",
    "profession": "profession",
    "interested_topics": ["topic1", "topic2"]
}}

Entity info:
{}"#, context
                ),
            },
        ];

        let result = self.llm.chat_json(&messages, 0.7, 2048).await?;

        Ok(AgentProfile {
            user_id: index + 1,
            user_name: entity.name.to_lowercase().replace(' ', "_").replace(|c: char| !c.is_alphanumeric() && c != '_', ""),
            name: result["name"].as_str().unwrap_or(&entity.name).to_string(),
            bio: result["bio"].as_str().unwrap_or(&entity.summary).to_string(),
            persona: result["persona"].as_str().unwrap_or("").to_string(),
            karma: 1000,
            friend_count: 100,
            follower_count: 150,
            statuses_count: 500,
            age: result["age"].as_i64().map(|v| v as i32),
            gender: result["gender"].as_str().map(|s| s.to_string()),
            mbti: result["mbti"].as_str().map(|s| s.to_string()),
            profession: result["profession"].as_str().map(|s| s.to_string()),
            interested_topics: result["interested_topics"].as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            source_entity_uuid: Some(entity.uuid.clone()),
            source_entity_type: Some(entity_type),
        })
    }

    pub fn save_profiles_json(profiles: &[AgentProfile], path: &std::path::Path) -> anyhow::Result<()> {
        let reddit_profiles: Vec<Value> = profiles.iter().map(|p| json!({
            "user_id": p.user_id,
            "username": p.user_name,
            "name": p.name,
            "bio": p.bio,
            "persona": p.persona,
            "karma": p.karma,
            "age": p.age,
            "gender": p.gender,
            "mbti": p.mbti,
            "profession": p.profession,
            "interested_topics": p.interested_topics,
        })).collect();
        let json = serde_json::to_string_pretty(&reddit_profiles)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn save_profiles_csv(profiles: &[AgentProfile], path: &std::path::Path) -> anyhow::Result<()> {
        let mut csv_lines = vec!["user_id,user_name,name,bio,persona,friend_count,follower_count,statuses_count".to_string()];
        for p in profiles {
            csv_lines.push(format!(
                "{},\"{}\",\"{}\",\"{}\",\"{}\",{},{},{}",
                p.user_id,
                p.user_name.replace('"', "\"\""),
                p.name.replace('"', "\"\""),
                p.bio.replace('"', "\"\""),
                p.persona.replace('"', "\"\""),
                p.friend_count, p.follower_count, p.statuses_count
            ));
        }
        std::fs::write(path, csv_lines.join("\n"))?;
        Ok(())
    }
}
