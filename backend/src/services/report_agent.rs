use crate::services::llm_client::{LLMClient, ChatMessage};
use crate::services::zep_client::ZepClient;
use crate::models::report::{Report, ReportStatus};
use serde_json::{json, Value};

pub struct ReportAgent {
    llm: LLMClient,
    zep: ZepClient,
    graph_id: String,
    simulation_id: String,
    simulation_requirement: String,
}

impl ReportAgent {
    pub fn new(
        llm: LLMClient, zep: ZepClient,
        graph_id: &str, simulation_id: &str,
        simulation_requirement: &str,
    ) -> Self {
        Self {
            llm, zep,
            graph_id: graph_id.to_string(),
            simulation_id: simulation_id.to_string(),
            simulation_requirement: simulation_requirement.to_string(),
        }
    }

    pub async fn generate_report(
        &self,
        report_id: Option<&str>,
        progress_callback: Option<&(dyn Fn(&str, i32, &str) + Send + Sync)>,
    ) -> anyhow::Result<Report> {
        let mut report = Report::new(&self.simulation_id, report_id);

        if let Some(cb) = progress_callback {
            cb("planning", 10, "Planning report structure...");
        }

        // Search graph for context
        let facts = self.zep.search_graph(&self.graph_id, &self.simulation_requirement, 20).await
            .unwrap_or_default();

        if let Some(cb) = progress_callback {
            cb("searching", 30, &format!("Found {} relevant facts", facts.len()));
        }

        // Generate report outline
        let outline = self.generate_outline(&facts).await?;
        report.outline = Some(outline.clone());

        if let Some(cb) = progress_callback {
            cb("generating", 50, "Generating report content...");
        }

        // Generate full report
        let content = self.generate_content(&outline, &facts).await?;
        report.markdown_content = content;

        if let Some(cb) = progress_callback {
            cb("finalizing", 90, "Finalizing report...");
        }

        report.status = ReportStatus::Completed;
        report.completed_at = Some(chrono::Local::now().to_rfc3339());

        Ok(report)
    }

    async fn generate_outline(&self, facts: &[String]) -> anyhow::Result<Value> {
        let facts_text = facts.iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n");

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are an expert analyst. Output a JSON report outline.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    r#"Create a report outline for this simulation analysis.

Requirement: {}

Key facts:
{}

Output JSON:
{{
    "title": "report title",
    "sections": [
        {{"title": "section title", "description": "what to cover"}}
    ]
}}"#,
                    self.simulation_requirement, facts_text
                ),
            },
        ];

        self.llm.chat_json(&messages, 0.5, 2048).await
    }

    async fn generate_content(&self, outline: &Value, facts: &[String]) -> anyhow::Result<String> {
        let facts_text = facts.iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n");

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are an expert analyst writing a detailed simulation analysis report in Markdown format.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "Write a detailed analysis report based on this outline and facts.\n\nOutline: {}\n\nFacts:\n{}\n\nRequirement: {}\n\nWrite the full report in Markdown format.",
                    serde_json::to_string_pretty(outline)?,
                    facts_text,
                    self.simulation_requirement
                ),
            },
        ];

        self.llm.chat(&messages, 0.5, 8192, None).await
    }

    pub async fn chat(
        &self, message: &str, chat_history: &[Value],
    ) -> anyhow::Result<Value> {
        // Search graph for relevant context
        let facts = self.zep.search_graph(&self.graph_id, message, 10).await
            .unwrap_or_default();

        let context = if facts.is_empty() {
            "No specific facts found.".to_string()
        } else {
            facts.join("\n")
        };

        let mut messages = vec![
            ChatMessage {
                role: "system".into(),
                content: format!(
                    "You are an AI analyst assistant for a social simulation. Use the following context to answer questions.\n\nContext:\n{}\n\nSimulation requirement: {}",
                    context, self.simulation_requirement
                ),
            },
        ];

        for msg in chat_history {
            if let (Some(role), Some(content)) = (msg["role"].as_str(), msg["content"].as_str()) {
                messages.push(ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                });
            }
        }

        messages.push(ChatMessage {
            role: "user".into(),
            content: message.to_string(),
        });

        let response = self.llm.chat(&messages, 0.7, 4096, None).await?;

        Ok(json!({
            "response": response,
            "sources": facts,
        }))
    }
}
