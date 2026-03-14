use crate::services::llm_client::{LLMClient, ChatMessage};
use serde_json::Value;

const ONTOLOGY_SYSTEM_PROMPT: &str = r#"You are a professional knowledge graph ontology design expert. Your task is to analyze the given text and simulation requirements, and design entity types and relationship types suitable for social media opinion simulation.

**IMPORTANT: You must output valid JSON format data, do not output any other content.**

## Output Format

Output JSON containing:

```json
{
    "entity_types": [
        {
            "name": "EntityTypeName (English, PascalCase)",
            "description": "Short description (English, max 100 chars)",
            "attributes": [
                {"name": "attr_name (snake_case)", "type": "text", "description": "..."}
            ],
            "examples": ["Example1", "Example2"]
        }
    ],
    "edge_types": [
        {
            "name": "RELATION_NAME (UPPER_SNAKE_CASE)",
            "description": "Short description (English, max 100 chars)",
            "source_targets": [{"source": "SourceType", "target": "TargetType"}],
            "attributes": []
        }
    ],
    "analysis_summary": "Brief analysis in the document's language"
}
```

## Design Guidelines

### Entity Types
- Exactly 10 entity types
- Last 2 must be fallback types: Person and Organization
- First 8 are specific types based on the text content
- All entities must be real-world subjects that can post on social media
- Cannot be abstract concepts, topics, or attitudes
- Attribute names cannot use: name, uuid, group_id, created_at, summary (reserved)

### Edge Types
- 6-10 relationship types
- Reflect real social media interactions

### Attributes
- 1-3 key attributes per entity type
"#;

pub struct OntologyGenerator {
    llm: LLMClient,
}

impl OntologyGenerator {
    pub fn new(llm: LLMClient) -> Self {
        Self { llm }
    }

    pub async fn generate(
        &self,
        document_texts: &[String],
        simulation_requirement: &str,
        additional_context: Option<&str>,
    ) -> anyhow::Result<Value> {
        let user_message = self.build_user_message(document_texts, simulation_requirement, additional_context);

        let messages = vec![
            ChatMessage { role: "system".into(), content: ONTOLOGY_SYSTEM_PROMPT.into() },
            ChatMessage { role: "user".into(), content: user_message },
        ];

        let mut result = self.llm.chat_json(&messages, 0.3, 4096).await?;
        self.validate_and_process(&mut result);
        Ok(result)
    }

    fn build_user_message(
        &self,
        document_texts: &[String],
        simulation_requirement: &str,
        additional_context: Option<&str>,
    ) -> String {
        let combined = document_texts.join("\n\n---\n\n");
        let max_len = 50000;
        let truncated = if combined.len() > max_len {
            format!("{}...(truncated)", &combined[..max_len])
        } else {
            combined
        };

        let mut msg = format!(
            "## Simulation Requirement\n\n{}\n\n## Document Content\n\n{}\n",
            simulation_requirement, truncated
        );

        if let Some(ctx) = additional_context {
            msg.push_str(&format!("\n## Additional Context\n\n{}\n", ctx));
        }

        msg.push_str("\nPlease design entity types and relationship types for social media simulation based on the above content.\n\n**Rules:**\n1. Exactly 10 entity types\n2. Last 2 must be Person (fallback) and Organization (fallback)\n3. First 8 are specific types based on text content\n4. All entities must be real-world subjects\n5. Attribute names cannot use reserved words\n");

        msg
    }

    fn validate_and_process(&self, result: &mut Value) {
        if result.get("entity_types").is_none() {
            result["entity_types"] = serde_json::json!([]);
        }
        if result.get("edge_types").is_none() {
            result["edge_types"] = serde_json::json!([]);
        }
        if result.get("analysis_summary").is_none() {
            result["analysis_summary"] = serde_json::json!("");
        }

        // Validate entity types
        if let Some(entities) = result["entity_types"].as_array_mut() {
            for entity in entities.iter_mut() {
                if entity.get("attributes").is_none() {
                    entity["attributes"] = serde_json::json!([]);
                }
                if entity.get("examples").is_none() {
                    entity["examples"] = serde_json::json!([]);
                }
                if let Some(desc) = entity["description"].as_str() {
                    if desc.len() > 100 {
                        let truncated = desc.chars().take(97).collect::<String>() + "...";
                        entity["description"] = serde_json::json!(truncated);
                    }
                }
            }

            // Ensure fallback types exist
            let names: Vec<String> = entities.iter()
                .filter_map(|e| e["name"].as_str().map(|s| s.to_string()))
                .collect();

            let has_person = names.contains(&"Person".to_string());
            let has_org = names.contains(&"Organization".to_string());

            if !has_person {
                entities.push(serde_json::json!({
                    "name": "Person",
                    "description": "Any individual person not fitting other specific person types.",
                    "attributes": [
                        {"name": "full_name", "type": "text", "description": "Full name"},
                        {"name": "role", "type": "text", "description": "Role or occupation"}
                    ],
                    "examples": ["ordinary citizen", "anonymous netizen"]
                }));
            }
            if !has_org {
                entities.push(serde_json::json!({
                    "name": "Organization",
                    "description": "Any organization not fitting other specific organization types.",
                    "attributes": [
                        {"name": "org_name", "type": "text", "description": "Name of the organization"},
                        {"name": "org_type", "type": "text", "description": "Type of organization"}
                    ],
                    "examples": ["small business", "community group"]
                }));
            }

            // Cap at 10
            entities.truncate(10);
        }

        // Validate edge types
        if let Some(edges) = result["edge_types"].as_array_mut() {
            for edge in edges.iter_mut() {
                if edge.get("source_targets").is_none() {
                    edge["source_targets"] = serde_json::json!([]);
                }
                if edge.get("attributes").is_none() {
                    edge["attributes"] = serde_json::json!([]);
                }
            }
            edges.truncate(10);
        }
    }
}
