//! Simulation Configuration Generator
//!
//! Uses LLM to intelligently generate detailed simulation parameters
//! based on simulation requirements, document content, and entity graph data.
//!
//! Implements a multi-step generation strategy to avoid overly long single-shot
//! LLM outputs:
//! 1. Generate time/scheduling configuration
//! 2. Generate event configuration and seed posts
//! 3. Generate agent configurations in batches
//! 4. Generate platform-specific settings
//! 5. Validate, apply defaults, and assemble final config

use crate::services::llm_client::{ChatMessage, LLMClient};
use crate::services::zep_entity_reader::EntityNode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tracing;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum character length for the combined context sent to the LLM.
const MAX_CONTEXT_LENGTH: usize = 50_000;

/// Number of agents to generate per LLM batch call.
const AGENTS_PER_BATCH: usize = 15;

/// Context truncation length for time-config prompts.
const TIME_CONFIG_CONTEXT_LENGTH: usize = 10_000;

/// Context truncation length for event-config prompts.
const EVENT_CONFIG_CONTEXT_LENGTH: usize = 8_000;

/// Max characters of each entity summary shown in the context.
const ENTITY_SUMMARY_LENGTH: usize = 300;

/// Max entities displayed per type in the overview.
const ENTITIES_PER_TYPE_DISPLAY: usize = 20;

/// Default LLM temperature for config generation.
const DEFAULT_TEMPERATURE: f64 = 0.7;

/// Default max tokens for LLM responses.
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Max LLM retry attempts.
const MAX_RETRY_ATTEMPTS: usize = 3;

// ---------------------------------------------------------------------------
// China Timezone Configuration
// ---------------------------------------------------------------------------

/// Activity multipliers for different times of day (Beijing Time).
struct ChinaTimezoneConfig;

impl ChinaTimezoneConfig {
    /// Hours considered "dead" (almost no activity).
    fn dead_hours() -> Vec<u32> {
        vec![0, 1, 2, 3, 4, 5]
    }
    /// Morning hours (gradually waking up).
    fn morning_hours() -> Vec<u32> {
        vec![6, 7, 8]
    }
    /// Working hours.
    fn work_hours() -> Vec<u32> {
        vec![9, 10, 11, 12, 13, 14, 15, 16, 17, 18]
    }
    /// Peak evening hours (most active).
    fn peak_hours() -> Vec<u32> {
        vec![19, 20, 21, 22]
    }
    /// Late night hours (declining activity).
    fn night_hours() -> Vec<u32> {
        vec![23]
    }

    fn dead_multiplier() -> f64 {
        0.05
    }
    fn morning_multiplier() -> f64 {
        0.4
    }
    fn work_multiplier() -> f64 {
        0.7
    }
    fn peak_multiplier() -> f64 {
        1.5
    }
    fn night_multiplier() -> f64 {
        0.5
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Per-agent activity configuration within the simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActivityConfig {
    pub agent_id: usize,
    pub entity_uuid: String,
    pub entity_name: String,
    pub entity_type: String,

    /// Overall activity level (0.0 - 1.0).
    #[serde(default = "default_activity_level")]
    pub activity_level: f64,

    /// Expected posts per simulated hour.
    #[serde(default = "default_posts_per_hour")]
    pub posts_per_hour: f64,

    /// Expected comments per simulated hour.
    #[serde(default = "default_comments_per_hour")]
    pub comments_per_hour: f64,

    /// Active hours in 24-hour format (0-23).
    #[serde(default = "default_active_hours")]
    pub active_hours: Vec<u32>,

    /// Minimum response delay in simulated minutes.
    #[serde(default = "default_response_delay_min")]
    pub response_delay_min: u32,

    /// Maximum response delay in simulated minutes.
    #[serde(default = "default_response_delay_max")]
    pub response_delay_max: u32,

    /// Sentiment bias (-1.0 = negative .. 1.0 = positive).
    #[serde(default)]
    pub sentiment_bias: f64,

    /// Stance toward the topic: supportive / opposing / neutral / observer.
    #[serde(default = "default_stance")]
    pub stance: String,

    /// Influence weight (determines visibility of this agent's posts to others).
    #[serde(default = "default_influence_weight")]
    pub influence_weight: f64,
}

fn default_activity_level() -> f64 {
    0.5
}
fn default_posts_per_hour() -> f64 {
    1.0
}
fn default_comments_per_hour() -> f64 {
    2.0
}
fn default_active_hours() -> Vec<u32> {
    (8..23).collect()
}
fn default_response_delay_min() -> u32 {
    5
}
fn default_response_delay_max() -> u32 {
    60
}
fn default_stance() -> String {
    "neutral".to_string()
}
fn default_influence_weight() -> f64 {
    1.0
}

/// Time simulation configuration (modeled after Chinese daily routine).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSimulationConfig {
    /// Total simulation duration in simulated hours (default 72 = 3 days).
    #[serde(default = "default_total_simulation_hours")]
    pub total_simulation_hours: u32,

    /// Simulated minutes per round (default 60 = 1 hour per round).
    #[serde(default = "default_minutes_per_round")]
    pub minutes_per_round: u32,

    /// Minimum number of agents activated per hour.
    #[serde(default = "default_agents_per_hour_min")]
    pub agents_per_hour_min: u32,

    /// Maximum number of agents activated per hour.
    #[serde(default = "default_agents_per_hour_max")]
    pub agents_per_hour_max: u32,

    /// Peak hours (evening, highest activity).
    #[serde(default = "default_peak_hours")]
    pub peak_hours: Vec<u32>,

    #[serde(default = "default_peak_activity_multiplier")]
    pub peak_activity_multiplier: f64,

    /// Off-peak (dead) hours.
    #[serde(default = "default_off_peak_hours")]
    pub off_peak_hours: Vec<u32>,

    #[serde(default = "default_off_peak_activity_multiplier")]
    pub off_peak_activity_multiplier: f64,

    /// Morning hours.
    #[serde(default = "default_morning_hours")]
    pub morning_hours: Vec<u32>,

    #[serde(default = "default_morning_activity_multiplier")]
    pub morning_activity_multiplier: f64,

    /// Work hours.
    #[serde(default = "default_work_hours")]
    pub work_hours: Vec<u32>,

    #[serde(default = "default_work_activity_multiplier")]
    pub work_activity_multiplier: f64,
}

fn default_total_simulation_hours() -> u32 {
    72
}
fn default_minutes_per_round() -> u32 {
    60
}
fn default_agents_per_hour_min() -> u32 {
    5
}
fn default_agents_per_hour_max() -> u32 {
    20
}
fn default_peak_hours() -> Vec<u32> {
    vec![19, 20, 21, 22]
}
fn default_peak_activity_multiplier() -> f64 {
    ChinaTimezoneConfig::peak_multiplier()
}
fn default_off_peak_hours() -> Vec<u32> {
    vec![0, 1, 2, 3, 4, 5]
}
fn default_off_peak_activity_multiplier() -> f64 {
    ChinaTimezoneConfig::dead_multiplier()
}
fn default_morning_hours() -> Vec<u32> {
    vec![6, 7, 8]
}
fn default_morning_activity_multiplier() -> f64 {
    ChinaTimezoneConfig::morning_multiplier()
}
fn default_work_hours() -> Vec<u32> {
    (9..19).collect()
}
fn default_work_activity_multiplier() -> f64 {
    ChinaTimezoneConfig::work_multiplier()
}

impl Default for TimeSimulationConfig {
    fn default() -> Self {
        Self {
            total_simulation_hours: default_total_simulation_hours(),
            minutes_per_round: default_minutes_per_round(),
            agents_per_hour_min: default_agents_per_hour_min(),
            agents_per_hour_max: default_agents_per_hour_max(),
            peak_hours: default_peak_hours(),
            peak_activity_multiplier: default_peak_activity_multiplier(),
            off_peak_hours: default_off_peak_hours(),
            off_peak_activity_multiplier: default_off_peak_activity_multiplier(),
            morning_hours: default_morning_hours(),
            morning_activity_multiplier: default_morning_activity_multiplier(),
            work_hours: default_work_hours(),
            work_activity_multiplier: default_work_activity_multiplier(),
        }
    }
}

/// Event configuration: initial seed posts, scheduled events, hot topics, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// Initial posts that kick off the simulation.
    #[serde(default)]
    pub initial_posts: Vec<Value>,

    /// Scheduled events triggered at specific simulation times.
    #[serde(default)]
    pub scheduled_events: Vec<Value>,

    /// Hot topic keywords.
    #[serde(default)]
    pub hot_topics: Vec<String>,

    /// Narrative direction / how public opinion should evolve.
    #[serde(default)]
    pub narrative_direction: String,
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            initial_posts: Vec::new(),
            scheduled_events: Vec::new(),
            hot_topics: Vec::new(),
            narrative_direction: String::new(),
        }
    }
}

/// Platform-specific configuration (Twitter or Reddit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub platform: String,

    /// Recency weight for recommendation algorithm.
    #[serde(default = "default_recency_weight")]
    pub recency_weight: f64,

    /// Popularity weight for recommendation algorithm.
    #[serde(default = "default_popularity_weight")]
    pub popularity_weight: f64,

    /// Relevance weight for recommendation algorithm.
    #[serde(default = "default_relevance_weight")]
    pub relevance_weight: f64,

    /// Interaction count at which a post "goes viral".
    #[serde(default = "default_viral_threshold")]
    pub viral_threshold: u32,

    /// Echo-chamber strength (how strongly similar opinions cluster).
    #[serde(default = "default_echo_chamber_strength")]
    pub echo_chamber_strength: f64,
}

fn default_recency_weight() -> f64 {
    0.4
}
fn default_popularity_weight() -> f64 {
    0.3
}
fn default_relevance_weight() -> f64 {
    0.3
}
fn default_viral_threshold() -> u32 {
    10
}
fn default_echo_chamber_strength() -> f64 {
    0.5
}

/// Complete simulation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationParameters {
    pub simulation_id: String,
    pub project_id: String,
    pub graph_id: String,
    pub simulation_requirement: String,

    /// Simulation topic (derived by LLM).
    #[serde(default)]
    pub simulation_topic: String,

    /// Background context summary.
    #[serde(default)]
    pub simulation_background: String,

    /// Maximum number of simulation rounds.
    #[serde(default)]
    pub max_rounds: usize,

    /// Whether Twitter platform is enabled.
    #[serde(default)]
    pub enable_twitter: bool,

    /// Whether Reddit platform is enabled.
    #[serde(default)]
    pub enable_reddit: bool,

    /// Time simulation configuration.
    #[serde(default)]
    pub time_config: TimeSimulationConfig,

    /// Per-agent activity configurations.
    #[serde(default)]
    pub agent_configs: Vec<AgentActivityConfig>,

    /// Event configuration (initial posts, hot topics, etc.).
    #[serde(default)]
    pub event_config: EventConfig,

    /// Twitter platform configuration.
    pub twitter_config: Option<PlatformConfig>,

    /// Reddit platform configuration.
    pub reddit_config: Option<PlatformConfig>,

    /// LLM model used for generation.
    #[serde(default)]
    pub llm_model: String,

    /// LLM base URL used for generation.
    #[serde(default)]
    pub llm_base_url: String,

    /// Timestamp of generation.
    #[serde(default)]
    pub generated_at: String,

    /// LLM's reasoning about why it chose these parameters.
    #[serde(default)]
    pub generation_reasoning: String,
}

impl SimulationParameters {
    /// Serialize to a pretty-printed JSON string.
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Convert to a serde_json::Value.
    pub fn to_value(&self) -> anyhow::Result<Value> {
        Ok(serde_json::to_value(self)?)
    }
}

// ---------------------------------------------------------------------------
// SimulationConfigGenerator
// ---------------------------------------------------------------------------

/// Intelligent simulation configuration generator.
///
/// Uses the LLM to analyze simulation requirements, document content, and graph
/// entities, then automatically produces optimal simulation parameters.
///
/// Generation is split into multiple smaller LLM calls:
/// 1. Time configuration (scheduling, rounds)
/// 2. Event configuration (seed posts, hot topics)
/// 3. Agent configurations (in batches of ~15)
/// 4. Platform configurations (Twitter/Reddit recommendation tuning)
pub struct SimulationConfigGenerator {
    llm: LLMClient,
}

impl SimulationConfigGenerator {
    pub fn new(llm: LLMClient) -> Self {
        Self { llm }
    }

    // -----------------------------------------------------------------------
    // Public entry point
    // -----------------------------------------------------------------------

    /// Generate a complete simulation configuration.
    ///
    /// This is the main entry point. It:
    /// 1. Builds comprehensive context from entities + document + requirement
    /// 2. Calls LLM in multiple steps to produce config sections
    /// 3. Parses JSON responses
    /// 4. Validates and post-processes
    /// 5. Applies defaults for any missing fields
    /// 6. Returns the assembled SimulationParameters as a serde_json::Value
    pub async fn generate_config(
        &self,
        simulation_requirement: &str,
        document_text: &str,
        entities: &[EntityNode],
        max_rounds: usize,
        enable_twitter: bool,
        enable_reddit: bool,
    ) -> anyhow::Result<Value> {
        let num_entities = entities.len();
        tracing::info!(
            "Starting simulation config generation: {} entities, max_rounds={}",
            num_entities,
            max_rounds,
        );

        // 1. Build the shared context string.
        let context = self.build_context(simulation_requirement, document_text, entities);

        let mut reasoning_parts: Vec<String> = Vec::new();

        // 2. Generate time configuration.
        tracing::info!("[1/4] Generating time configuration...");
        let time_config_raw = self.generate_time_config(&context, num_entities).await;
        let time_config = self.parse_time_config(&time_config_raw, num_entities);
        if let Some(r) = time_config_raw.get("reasoning").and_then(|v| v.as_str()) {
            reasoning_parts.push(format!("Time config: {}", r));
        }

        // 3. Generate event configuration (initial posts, hot topics).
        tracing::info!("[2/4] Generating event configuration and hot topics...");
        let event_config_raw = self
            .generate_event_config(&context, simulation_requirement, entities)
            .await;
        let event_config = self.parse_event_config(&event_config_raw);
        if let Some(r) = event_config_raw.get("reasoning").and_then(|v| v.as_str()) {
            reasoning_parts.push(format!("Event config: {}", r));
        }

        // 4. Generate agent configurations in batches.
        tracing::info!("[3/4] Generating agent configurations in batches...");
        let num_batches = if num_entities == 0 {
            0
        } else {
            (num_entities + AGENTS_PER_BATCH - 1) / AGENTS_PER_BATCH
        };

        let mut all_agent_configs: Vec<AgentActivityConfig> = Vec::new();
        for batch_idx in 0..num_batches {
            let start = batch_idx * AGENTS_PER_BATCH;
            let end = std::cmp::min(start + AGENTS_PER_BATCH, num_entities);
            let batch_entities = &entities[start..end];

            tracing::info!(
                "  Generating agent configs batch {}/{} (agents {}-{})...",
                batch_idx + 1,
                num_batches,
                start + 1,
                end,
            );

            let batch_configs = self
                .generate_agent_configs_batch(&context, batch_entities, start, simulation_requirement)
                .await;
            all_agent_configs.extend(batch_configs);
        }
        reasoning_parts.push(format!(
            "Agent configs: successfully generated {} agents",
            all_agent_configs.len()
        ));

        // 5. Assign poster agents to initial posts.
        tracing::info!("Assigning poster agents to initial posts...");
        let event_config = self.assign_initial_post_agents(event_config, &all_agent_configs);

        // 6. Build platform configs.
        tracing::info!("[4/4] Building platform configurations...");
        let twitter_config = if enable_twitter {
            Some(PlatformConfig {
                platform: "twitter".to_string(),
                recency_weight: 0.4,
                popularity_weight: 0.3,
                relevance_weight: 0.3,
                viral_threshold: 10,
                echo_chamber_strength: 0.5,
            })
        } else {
            None
        };

        let reddit_config = if enable_reddit {
            Some(PlatformConfig {
                platform: "reddit".to_string(),
                recency_weight: 0.3,
                popularity_weight: 0.4,
                relevance_weight: 0.3,
                viral_threshold: 15,
                echo_chamber_strength: 0.6,
            })
        } else {
            None
        };

        // 7. Derive simulation topic from event config.
        let simulation_topic = event_config
            .hot_topics
            .first()
            .cloned()
            .unwrap_or_else(|| simulation_requirement.chars().take(100).collect());

        // 8. Assemble final parameters.
        let params = SimulationParameters {
            simulation_id: String::new(),
            project_id: String::new(),
            graph_id: String::new(),
            simulation_requirement: simulation_requirement.to_string(),
            simulation_topic,
            simulation_background: event_config.narrative_direction.clone(),
            max_rounds,
            enable_twitter,
            enable_reddit,
            time_config,
            agent_configs: all_agent_configs,
            event_config,
            twitter_config,
            reddit_config,
            llm_model: String::new(),
            llm_base_url: String::new(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            generation_reasoning: reasoning_parts.join(" | "),
        };

        // 9. Validate final config.
        self.validate_config(&params);

        tracing::info!(
            "Simulation config generation complete: {} agent configs",
            params.agent_configs.len(),
        );

        params.to_value()
    }

    // -----------------------------------------------------------------------
    // Context building
    // -----------------------------------------------------------------------

    /// Build the comprehensive LLM context from requirement, document, and entities.
    fn build_context(
        &self,
        simulation_requirement: &str,
        document_text: &str,
        entities: &[EntityNode],
    ) -> String {
        let entity_summary = self.summarize_entities(entities);

        let mut parts = vec![
            format!("## Simulation Requirement\n{}", simulation_requirement),
            format!(
                "\n## Entity Information ({} entities)\n{}",
                entities.len(),
                entity_summary
            ),
        ];

        let current_len: usize = parts.iter().map(|p| p.len()).sum();
        let remaining = MAX_CONTEXT_LENGTH.saturating_sub(current_len + 500);

        if remaining > 0 && !document_text.is_empty() {
            let doc_truncated = if document_text.len() > remaining {
                format!(
                    "{}\n...(document truncated)",
                    safe_truncate(document_text, remaining)
                )
            } else {
                document_text.to_string()
            };
            parts.push(format!("\n## Original Document Content\n{}", doc_truncated));
        }

        parts.join("\n")
    }

    /// Summarize entities grouped by type for the LLM context.
    fn summarize_entities(&self, entities: &[EntityNode]) -> String {
        let mut by_type: HashMap<String, Vec<&EntityNode>> = HashMap::new();
        for e in entities {
            let t = e.get_entity_type().unwrap_or_else(|| "Unknown".to_string());
            by_type.entry(t).or_default().push(e);
        }

        let mut lines: Vec<String> = Vec::new();
        for (entity_type, type_entities) in &by_type {
            lines.push(format!(
                "\n### {} ({} entities)",
                entity_type,
                type_entities.len()
            ));
            for e in type_entities.iter().take(ENTITIES_PER_TYPE_DISPLAY) {
                let summary_preview = if e.summary.len() > ENTITY_SUMMARY_LENGTH {
                    format!("{}...", safe_truncate(&e.summary, ENTITY_SUMMARY_LENGTH))
                } else {
                    e.summary.clone()
                };
                lines.push(format!("- {}: {}", e.name, summary_preview));
            }
            if type_entities.len() > ENTITIES_PER_TYPE_DISPLAY {
                lines.push(format!(
                    "  ... and {} more",
                    type_entities.len() - ENTITIES_PER_TYPE_DISPLAY
                ));
            }
        }

        lines.join("\n")
    }

    // -----------------------------------------------------------------------
    // Step 1: Time configuration
    // -----------------------------------------------------------------------

    /// Generate time/scheduling configuration via LLM.
    async fn generate_time_config(&self, context: &str, num_entities: usize) -> Value {
        let context_truncated = safe_truncate(context, TIME_CONFIG_CONTEXT_LENGTH);
        let max_agents_allowed = std::cmp::max(1, (num_entities as f64 * 0.9) as u32);

        let prompt = format!(
            r#"Based on the following simulation requirements, generate time simulation configuration.

{context_truncated}

## Task
Generate a time configuration JSON.

### Guiding Principles (adapt based on the specific scenario):
- Target user group follows Beijing time daily routines
- 0-5 AM: almost no activity (activity multiplier ~0.05)
- 6-8 AM: gradually becoming active (multiplier ~0.4)
- 9 AM-6 PM: moderate activity during work hours (multiplier ~0.7)
- 7-10 PM: peak hours, highest activity (multiplier ~1.5)
- 11 PM: declining activity (multiplier ~0.5)
- Adjust time windows based on the nature of the event and participant demographics
  - Students may peak at 9-11 PM; media active all day; officials only during work hours
  - Breaking news may cause late-night discussion spikes

### Return JSON format (no markdown):
{{
    "total_simulation_hours": <24-168>,
    "minutes_per_round": <30-120, recommend 60>,
    "agents_per_hour_min": <1-{max_agents_allowed}>,
    "agents_per_hour_max": <1-{max_agents_allowed}>,
    "peak_hours": [19, 20, 21, 22],
    "off_peak_hours": [0, 1, 2, 3, 4, 5],
    "morning_hours": [6, 7, 8],
    "work_hours": [9, 10, 11, 12, 13, 14, 15, 16, 17, 18],
    "reasoning": "Brief explanation of configuration choices"
}}"#
        );

        let system_prompt =
            "You are a social media simulation expert. Return pure JSON. \
             Time configuration should follow Chinese daily routine patterns.";

        match self.call_llm_with_retry(&prompt, system_prompt).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Time config LLM generation failed: {}, using defaults", e);
                self.get_default_time_config(num_entities)
            }
        }
    }

    /// Build a default time configuration when LLM fails.
    fn get_default_time_config(&self, num_entities: usize) -> Value {
        let n = num_entities as u32;
        json!({
            "total_simulation_hours": 72,
            "minutes_per_round": 60,
            "agents_per_hour_min": std::cmp::max(1, n / 15),
            "agents_per_hour_max": std::cmp::max(5, n / 5),
            "peak_hours": [19, 20, 21, 22],
            "off_peak_hours": [0, 1, 2, 3, 4, 5],
            "morning_hours": [6, 7, 8],
            "work_hours": [9, 10, 11, 12, 13, 14, 15, 16, 17, 18],
            "reasoning": "Default Chinese daily routine configuration (1 hour per round)"
        })
    }

    /// Parse time config JSON into a TimeSimulationConfig, clamping values.
    fn parse_time_config(&self, result: &Value, num_entities: usize) -> TimeSimulationConfig {
        let n = num_entities as u32;

        let mut agents_min = result["agents_per_hour_min"]
            .as_u64()
            .unwrap_or(std::cmp::max(1, n as u64 / 15)) as u32;
        let mut agents_max = result["agents_per_hour_max"]
            .as_u64()
            .unwrap_or(std::cmp::max(5, n as u64 / 5)) as u32;

        // Clamp to total entity count.
        if agents_min > n {
            tracing::warn!(
                "agents_per_hour_min ({}) exceeds total agents ({}), clamping",
                agents_min,
                n
            );
            agents_min = std::cmp::max(1, n / 10);
        }
        if agents_max > n {
            tracing::warn!(
                "agents_per_hour_max ({}) exceeds total agents ({}), clamping",
                agents_max,
                n
            );
            agents_max = std::cmp::max(agents_min + 1, n / 2);
        }
        if agents_min >= agents_max {
            agents_min = std::cmp::max(1, agents_max / 2);
            tracing::warn!(
                "agents_per_hour_min >= max, corrected min to {}",
                agents_min
            );
        }

        let parse_hours = |key: &str, default: Vec<u32>| -> Vec<u32> {
            result[key]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_u64().map(|x| x as u32))
                        .collect()
                })
                .unwrap_or(default)
        };

        TimeSimulationConfig {
            total_simulation_hours: result["total_simulation_hours"]
                .as_u64()
                .unwrap_or(72) as u32,
            minutes_per_round: result["minutes_per_round"].as_u64().unwrap_or(60) as u32,
            agents_per_hour_min: agents_min,
            agents_per_hour_max: agents_max,
            peak_hours: parse_hours("peak_hours", ChinaTimezoneConfig::peak_hours()),
            peak_activity_multiplier: ChinaTimezoneConfig::peak_multiplier(),
            off_peak_hours: parse_hours("off_peak_hours", ChinaTimezoneConfig::dead_hours()),
            off_peak_activity_multiplier: ChinaTimezoneConfig::dead_multiplier(),
            morning_hours: parse_hours("morning_hours", ChinaTimezoneConfig::morning_hours()),
            morning_activity_multiplier: ChinaTimezoneConfig::morning_multiplier(),
            work_hours: parse_hours("work_hours", ChinaTimezoneConfig::work_hours()),
            work_activity_multiplier: ChinaTimezoneConfig::work_multiplier(),
        }
    }

    // -----------------------------------------------------------------------
    // Step 2: Event configuration
    // -----------------------------------------------------------------------

    /// Generate event configuration (initial posts, hot topics) via LLM.
    async fn generate_event_config(
        &self,
        context: &str,
        simulation_requirement: &str,
        entities: &[EntityNode],
    ) -> Value {
        // Collect available entity types and representative names.
        let mut type_examples: HashMap<String, Vec<String>> = HashMap::new();
        for e in entities {
            let etype = e.get_entity_type().unwrap_or_else(|| "Unknown".to_string());
            let names = type_examples.entry(etype).or_default();
            if names.len() < 3 {
                names.push(e.name.clone());
            }
        }

        let type_info: String = type_examples
            .iter()
            .map(|(t, examples)| format!("- {}: {}", t, examples.join(", ")))
            .collect::<Vec<_>>()
            .join("\n");

        let context_truncated = safe_truncate(context, EVENT_CONFIG_CONTEXT_LENGTH);

        let prompt = format!(
            r#"Based on the following simulation requirements, generate event configuration.

Simulation requirement: {simulation_requirement}

{context_truncated}

## Available Entity Types and Examples
{type_info}

## Task
Generate event configuration JSON:
- Extract hot topic keywords
- Describe the direction of public opinion evolution
- Design initial seed posts. Each post MUST specify poster_type (publisher type)

IMPORTANT: poster_type must be chosen from the "Available Entity Types" above so that
initial posts can be assigned to appropriate agents.
For example: official announcements should use Official/University type,
news by MediaOutlet, student opinions by Student, etc.

Return JSON format (no markdown):
{{
    "hot_topics": ["keyword1", "keyword2", ...],
    "narrative_direction": "<description of how public opinion evolves>",
    "initial_posts": [
        {{"content": "Post content text", "poster_type": "EntityType (must match available types)"}},
        ...
    ],
    "reasoning": "<brief explanation>"
}}"#
        );

        let system_prompt =
            "You are a public opinion analysis expert. Return pure JSON. \
             Make sure poster_type exactly matches available entity types.";

        match self.call_llm_with_retry(&prompt, system_prompt).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Event config LLM generation failed: {}, using defaults", e);
                json!({
                    "hot_topics": [],
                    "narrative_direction": "",
                    "initial_posts": [],
                    "reasoning": "Default configuration"
                })
            }
        }
    }

    /// Parse event configuration JSON into an EventConfig struct.
    fn parse_event_config(&self, result: &Value) -> EventConfig {
        let initial_posts = result["initial_posts"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let hot_topics = result["hot_topics"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let narrative_direction = result["narrative_direction"]
            .as_str()
            .unwrap_or("")
            .to_string();

        EventConfig {
            initial_posts,
            scheduled_events: Vec::new(),
            hot_topics,
            narrative_direction,
        }
    }

    // -----------------------------------------------------------------------
    // Step 3: Agent configurations (batched)
    // -----------------------------------------------------------------------

    /// Generate agent activity configurations for a batch of entities.
    async fn generate_agent_configs_batch(
        &self,
        _context: &str,
        entities: &[EntityNode],
        start_idx: usize,
        simulation_requirement: &str,
    ) -> Vec<AgentActivityConfig> {
        // Build entity list for the prompt.
        let entity_list: Vec<Value> = entities
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let summary = safe_truncate(&e.summary, ENTITY_SUMMARY_LENGTH);
                json!({
                    "agent_id": start_idx + i,
                    "entity_name": e.name,
                    "entity_type": e.get_entity_type().unwrap_or_else(|| "Unknown".to_string()),
                    "summary": summary
                })
            })
            .collect();

        let entity_json = serde_json::to_string_pretty(&entity_list).unwrap_or_default();

        let prompt = format!(
            r#"Based on the following information, generate social media activity configuration for each entity.

Simulation requirement: {simulation_requirement}

## Entity List
```json
{entity_json}
```

## Task
Generate activity configuration for each entity. Guidelines:
- **Time follows Chinese daily routine**: 0-5 AM almost no activity, 7-10 PM most active
- **Official institutions** (University/GovernmentAgency): low activity (0.1-0.3), work hours (9-17), slow response (60-240 min), high influence (2.5-3.0)
- **Media** (MediaOutlet): medium activity (0.4-0.6), all day (8-23), fast response (5-30 min), high influence (2.0-2.5)
- **Individuals** (Student/Person/Alumni): high activity (0.6-0.9), mainly evening (18-23), fast response (1-15 min), low influence (0.8-1.2)
- **Public figures/Experts** (Professor): medium activity (0.4-0.6), medium-high influence (1.5-2.0)

Return JSON format (no markdown):
{{
    "agent_configs": [
        {{
            "agent_id": <must match input>,
            "activity_level": <0.0-1.0>,
            "posts_per_hour": <posting frequency>,
            "comments_per_hour": <commenting frequency>,
            "active_hours": [<list of active hours considering Chinese routine>],
            "response_delay_min": <minimum response delay in minutes>,
            "response_delay_max": <maximum response delay in minutes>,
            "sentiment_bias": <-1.0 to 1.0>,
            "stance": "<supportive/opposing/neutral/observer>",
            "influence_weight": <influence weight>
        }},
        ...
    ]
}}"#
        );

        let system_prompt =
            "You are a social media behavior analysis expert. Return pure JSON. \
             Configuration should follow Chinese daily routine patterns.";

        // Try LLM, fall back to rule-based generation.
        let llm_configs: HashMap<usize, Value> = match self
            .call_llm_with_retry(&prompt, system_prompt)
            .await
        {
            Ok(result) => result["agent_configs"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|cfg| {
                            cfg["agent_id"].as_u64().map(|id| (id as usize, cfg.clone()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Err(e) => {
                tracing::warn!(
                    "Agent config batch LLM generation failed: {}, using rule-based fallback",
                    e
                );
                HashMap::new()
            }
        };

        // Build AgentActivityConfig objects, using LLM data if available, else rules.
        let mut configs = Vec::new();
        for (i, entity) in entities.iter().enumerate() {
            let agent_id = start_idx + i;
            let entity_type = entity
                .get_entity_type()
                .unwrap_or_else(|| "Unknown".to_string());

            let cfg = if let Some(llm_cfg) = llm_configs.get(&agent_id) {
                llm_cfg.clone()
            } else {
                self.generate_agent_config_by_rule(&entity_type)
            };

            let active_hours: Vec<u32> = cfg["active_hours"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_u64().map(|x| x as u32))
                        .collect()
                })
                .unwrap_or_else(|| (9..23).collect());

            let config = AgentActivityConfig {
                agent_id,
                entity_uuid: entity.uuid.clone(),
                entity_name: entity.name.clone(),
                entity_type: entity_type.clone(),
                activity_level: cfg["activity_level"].as_f64().unwrap_or(0.5),
                posts_per_hour: cfg["posts_per_hour"].as_f64().unwrap_or(0.5),
                comments_per_hour: cfg["comments_per_hour"].as_f64().unwrap_or(1.0),
                active_hours,
                response_delay_min: cfg["response_delay_min"].as_u64().unwrap_or(5) as u32,
                response_delay_max: cfg["response_delay_max"].as_u64().unwrap_or(60) as u32,
                sentiment_bias: cfg["sentiment_bias"].as_f64().unwrap_or(0.0),
                stance: cfg["stance"]
                    .as_str()
                    .unwrap_or("neutral")
                    .to_string(),
                influence_weight: cfg["influence_weight"].as_f64().unwrap_or(1.0),
            };
            configs.push(config);
        }

        configs
    }

    /// Rule-based agent configuration fallback, by entity type (Chinese routine).
    fn generate_agent_config_by_rule(&self, entity_type: &str) -> Value {
        let et = entity_type.to_lowercase();
        match et.as_str() {
            "university" | "governmentagency" | "ngo" => json!({
                "activity_level": 0.2,
                "posts_per_hour": 0.1,
                "comments_per_hour": 0.05,
                "active_hours": (9u32..18).collect::<Vec<u32>>(),
                "response_delay_min": 60,
                "response_delay_max": 240,
                "sentiment_bias": 0.0,
                "stance": "neutral",
                "influence_weight": 3.0
            }),
            "mediaoutlet" | "media" => json!({
                "activity_level": 0.5,
                "posts_per_hour": 0.8,
                "comments_per_hour": 0.3,
                "active_hours": (7u32..24).collect::<Vec<u32>>(),
                "response_delay_min": 5,
                "response_delay_max": 30,
                "sentiment_bias": 0.0,
                "stance": "observer",
                "influence_weight": 2.5
            }),
            "professor" | "expert" | "official" => json!({
                "activity_level": 0.4,
                "posts_per_hour": 0.3,
                "comments_per_hour": 0.5,
                "active_hours": (8u32..22).collect::<Vec<u32>>(),
                "response_delay_min": 15,
                "response_delay_max": 90,
                "sentiment_bias": 0.0,
                "stance": "neutral",
                "influence_weight": 2.0
            }),
            "student" => json!({
                "activity_level": 0.8,
                "posts_per_hour": 0.6,
                "comments_per_hour": 1.5,
                "active_hours": vec![8u32, 9, 10, 11, 12, 13, 18, 19, 20, 21, 22, 23],
                "response_delay_min": 1,
                "response_delay_max": 15,
                "sentiment_bias": 0.0,
                "stance": "neutral",
                "influence_weight": 0.8
            }),
            "alumni" => json!({
                "activity_level": 0.6,
                "posts_per_hour": 0.4,
                "comments_per_hour": 0.8,
                "active_hours": vec![12u32, 13, 19, 20, 21, 22, 23],
                "response_delay_min": 5,
                "response_delay_max": 30,
                "sentiment_bias": 0.0,
                "stance": "neutral",
                "influence_weight": 1.0
            }),
            _ => json!({
                "activity_level": 0.7,
                "posts_per_hour": 0.5,
                "comments_per_hour": 1.2,
                "active_hours": vec![9u32, 10, 11, 12, 13, 18, 19, 20, 21, 22, 23],
                "response_delay_min": 2,
                "response_delay_max": 20,
                "sentiment_bias": 0.0,
                "stance": "neutral",
                "influence_weight": 1.0
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Post-processing: assign poster agents to initial posts
    // -----------------------------------------------------------------------

    /// Assign an appropriate agent_id to each initial post based on poster_type.
    fn assign_initial_post_agents(
        &self,
        mut event_config: EventConfig,
        agent_configs: &[AgentActivityConfig],
    ) -> EventConfig {
        if event_config.initial_posts.is_empty() || agent_configs.is_empty() {
            return event_config;
        }

        // Build index by entity type (lowercase).
        let mut agents_by_type: HashMap<String, Vec<&AgentActivityConfig>> = HashMap::new();
        for agent in agent_configs {
            let etype = agent.entity_type.to_lowercase();
            agents_by_type.entry(etype).or_default().push(agent);
        }

        // Alias mapping for flexible type matching.
        let type_aliases: HashMap<&str, Vec<&str>> = HashMap::from([
            (
                "official",
                vec!["official", "university", "governmentagency", "government"],
            ),
            ("university", vec!["university", "official"]),
            ("mediaoutlet", vec!["mediaoutlet", "media"]),
            ("student", vec!["student", "person"]),
            ("professor", vec!["professor", "expert", "teacher"]),
            ("alumni", vec!["alumni", "person"]),
            (
                "organization",
                vec!["organization", "ngo", "company", "group"],
            ),
            ("person", vec!["person", "student", "alumni"]),
        ]);

        // Track usage indices per type to distribute posts across agents.
        let mut used_indices: HashMap<String, usize> = HashMap::new();

        let mut updated_posts: Vec<Value> = Vec::new();

        for post in &event_config.initial_posts {
            let poster_type = post["poster_type"]
                .as_str()
                .unwrap_or("")
                .to_lowercase();
            let content = post["content"].as_str().unwrap_or("").to_string();

            let mut matched_agent_id: Option<usize> = None;

            // 1. Direct match.
            if let Some(agents) = agents_by_type.get(&poster_type) {
                let idx = *used_indices.get(&poster_type).unwrap_or(&0) % agents.len();
                matched_agent_id = Some(agents[idx].agent_id);
                used_indices.insert(poster_type.clone(), idx + 1);
            }

            // 2. Alias match.
            if matched_agent_id.is_none() {
                'outer: for (alias_key, aliases) in &type_aliases {
                    if poster_type == *alias_key || aliases.contains(&poster_type.as_str()) {
                        for alias in aliases {
                            let alias_str = alias.to_string();
                            if let Some(agents) = agents_by_type.get(&alias_str) {
                                let idx =
                                    *used_indices.get(&alias_str).unwrap_or(&0) % agents.len();
                                matched_agent_id = Some(agents[idx].agent_id);
                                used_indices.insert(alias_str, idx + 1);
                                break 'outer;
                            }
                        }
                    }
                }
            }

            // 3. Fallback: use the agent with highest influence.
            if matched_agent_id.is_none() {
                tracing::warn!(
                    "No agent match for poster_type '{}', using highest-influence agent",
                    poster_type
                );
                matched_agent_id = agent_configs
                    .iter()
                    .max_by(|a, b| {
                        a.influence_weight
                            .partial_cmp(&b.influence_weight)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|a| a.agent_id);
            }

            updated_posts.push(json!({
                "content": content,
                "poster_type": post["poster_type"].as_str().unwrap_or("Unknown"),
                "poster_agent_id": matched_agent_id.unwrap_or(0),
            }));

            tracing::info!(
                "Initial post assignment: poster_type='{}' -> agent_id={}",
                poster_type,
                matched_agent_id.unwrap_or(0)
            );
        }

        event_config.initial_posts = updated_posts;
        event_config
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    /// Validate all config values are within acceptable ranges; log warnings
    /// for any out-of-range values (non-fatal).
    fn validate_config(&self, params: &SimulationParameters) {
        // Validate time config.
        let tc = &params.time_config;
        if tc.total_simulation_hours == 0 {
            tracing::warn!("total_simulation_hours is 0, simulation will be empty");
        }
        if tc.minutes_per_round == 0 {
            tracing::warn!("minutes_per_round is 0, may cause division by zero");
        }
        if tc.agents_per_hour_min > tc.agents_per_hour_max {
            tracing::warn!(
                "agents_per_hour_min ({}) > agents_per_hour_max ({})",
                tc.agents_per_hour_min,
                tc.agents_per_hour_max
            );
        }

        // Validate agent configs.
        for agent in &params.agent_configs {
            if agent.activity_level < 0.0 || agent.activity_level > 1.0 {
                tracing::warn!(
                    "Agent {} ({}) activity_level {} out of range [0, 1]",
                    agent.agent_id,
                    agent.entity_name,
                    agent.activity_level
                );
            }
            if agent.sentiment_bias < -1.0 || agent.sentiment_bias > 1.0 {
                tracing::warn!(
                    "Agent {} ({}) sentiment_bias {} out of range [-1, 1]",
                    agent.agent_id,
                    agent.entity_name,
                    agent.sentiment_bias
                );
            }
            if agent.influence_weight < 0.0 {
                tracing::warn!(
                    "Agent {} ({}) influence_weight {} is negative",
                    agent.agent_id,
                    agent.entity_name,
                    agent.influence_weight
                );
            }
            if agent.response_delay_min > agent.response_delay_max {
                tracing::warn!(
                    "Agent {} ({}) response_delay_min ({}) > response_delay_max ({})",
                    agent.agent_id,
                    agent.entity_name,
                    agent.response_delay_min,
                    agent.response_delay_max
                );
            }
            let valid_stances = ["supportive", "opposing", "neutral", "observer"];
            if !valid_stances.contains(&agent.stance.as_str()) {
                tracing::warn!(
                    "Agent {} ({}) stance '{}' not in {:?}",
                    agent.agent_id,
                    agent.entity_name,
                    agent.stance,
                    valid_stances
                );
            }
        }

        // Validate platform configs.
        if let Some(pc) = &params.twitter_config {
            self.validate_platform_config("twitter", pc);
        }
        if let Some(pc) = &params.reddit_config {
            self.validate_platform_config("reddit", pc);
        }
    }

    /// Validate a single platform configuration.
    fn validate_platform_config(&self, name: &str, pc: &PlatformConfig) {
        let weight_sum = pc.recency_weight + pc.popularity_weight + pc.relevance_weight;
        if (weight_sum - 1.0).abs() > 0.1 {
            tracing::warn!(
                "Platform '{}' recommendation weights sum to {} (expected ~1.0)",
                name,
                weight_sum
            );
        }
        if pc.echo_chamber_strength < 0.0 || pc.echo_chamber_strength > 1.0 {
            tracing::warn!(
                "Platform '{}' echo_chamber_strength {} out of range [0, 1]",
                name,
                pc.echo_chamber_strength
            );
        }
    }

    // -----------------------------------------------------------------------
    // LLM calling with retry and JSON repair
    // -----------------------------------------------------------------------

    /// Call the LLM with retry logic and truncated-JSON repair.
    async fn call_llm_with_retry(
        &self,
        prompt: &str,
        system_prompt: &str,
    ) -> anyhow::Result<Value> {
        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 0..MAX_RETRY_ATTEMPTS {
            let temperature = DEFAULT_TEMPERATURE - (attempt as f64 * 0.1);

            let messages = vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                },
            ];

            match self
                .llm
                .chat_json(&messages, temperature, DEFAULT_MAX_TOKENS)
                .await
            {
                Ok(value) => return Ok(value),
                Err(e) => {
                    tracing::warn!(
                        "LLM call failed (attempt {}/{}): {}",
                        attempt + 1,
                        MAX_RETRY_ATTEMPTS,
                        e
                    );

                    // Try to extract and repair JSON from the error message.
                    let err_str = e.to_string();
                    if let Some(repaired) = self.try_repair_json_from_error(&err_str) {
                        tracing::info!("Successfully repaired JSON from error output");
                        return Ok(repaired);
                    }

                    last_error = Some(e);

                    // Brief delay before retry (increasing backoff).
                    if attempt + 1 < MAX_RETRY_ATTEMPTS {
                        tokio::time::sleep(std::time::Duration::from_secs(
                            2 * (attempt as u64 + 1),
                        ))
                        .await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("LLM call failed after retries")))
    }

    /// Attempt to repair a truncated or malformed JSON string.
    fn fix_truncated_json(&self, content: &str) -> String {
        let mut s = content.trim().to_string();

        // Count unclosed brackets and braces.
        let open_braces = s.matches('{').count() as i32 - s.matches('}').count() as i32;
        let open_brackets = s.matches('[').count() as i32 - s.matches(']').count() as i32;

        // If the string ends mid-value, try to close the current string.
        if let Some(last_char) = s.chars().last() {
            if last_char != '"' && last_char != '}' && last_char != ']' && last_char != ',' {
                s.push('"');
            }
        }

        // Close open brackets.
        for _ in 0..open_brackets.max(0) {
            s.push(']');
        }
        // Close open braces.
        for _ in 0..open_braces.max(0) {
            s.push('}');
        }

        s
    }

    /// Try to extract and parse JSON from an error message that may contain
    /// partial LLM output.
    fn try_repair_json_from_error(&self, error_msg: &str) -> Option<Value> {
        // Look for JSON object in the error text.
        let start = error_msg.find('{')?;
        let candidate = &error_msg[start..];

        // Try direct parse first.
        if let Ok(v) = serde_json::from_str::<Value>(candidate) {
            return Some(v);
        }

        // Try repair.
        let fixed = self.fix_truncated_json(candidate);
        serde_json::from_str::<Value>(&fixed).ok()
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Safely truncate a string to at most `max_len` bytes on a char boundary.
fn safe_truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    // Find the last char boundary at or before max_len.
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ---------------------------------------------------------------------------
// Helper: calculate_activity_frequency
// ---------------------------------------------------------------------------

/// Map an activity level label ("high"/"medium"/"low") to a numeric frequency.
pub fn calculate_activity_frequency(level: &str) -> f64 {
    match level.to_lowercase().as_str() {
        "high" => 0.8,
        "medium" => 0.5,
        "low" => 0.2,
        _ => 0.5,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_truncate_ascii() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
        assert_eq!(safe_truncate("hi", 10), "hi");
    }

    #[test]
    fn test_safe_truncate_multibyte() {
        let s = "hello\u{4e16}\u{754c}"; // "hello世界"
        let t = safe_truncate(s, 6);
        // Should not panic, should end on a char boundary.
        assert!(t.len() <= 6);
        assert!(t.is_char_boundary(t.len()));
    }

    #[test]
    fn test_calculate_activity_frequency() {
        assert!((calculate_activity_frequency("high") - 0.8).abs() < f64::EPSILON);
        assert!((calculate_activity_frequency("medium") - 0.5).abs() < f64::EPSILON);
        assert!((calculate_activity_frequency("low") - 0.2).abs() < f64::EPSILON);
        assert!((calculate_activity_frequency("unknown") - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_time_config() {
        let cfg = TimeSimulationConfig::default();
        assert_eq!(cfg.total_simulation_hours, 72);
        assert_eq!(cfg.minutes_per_round, 60);
        assert_eq!(cfg.peak_hours, vec![19, 20, 21, 22]);
        assert_eq!(cfg.off_peak_hours, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_simulation_parameters_serialization() {
        let params = SimulationParameters {
            simulation_id: "test-id".to_string(),
            project_id: "proj-1".to_string(),
            graph_id: "graph-1".to_string(),
            simulation_requirement: "test requirement".to_string(),
            simulation_topic: "test topic".to_string(),
            simulation_background: String::new(),
            max_rounds: 10,
            enable_twitter: true,
            enable_reddit: false,
            time_config: TimeSimulationConfig::default(),
            agent_configs: vec![],
            event_config: EventConfig::default(),
            twitter_config: None,
            reddit_config: None,
            llm_model: String::new(),
            llm_base_url: String::new(),
            generated_at: String::new(),
            generation_reasoning: "test reasoning".to_string(),
        };

        let json_str = params.to_json().unwrap();
        assert!(json_str.contains("test-id"));
        assert!(json_str.contains("test reasoning"));

        let value = params.to_value().unwrap();
        assert_eq!(value["generation_reasoning"], "test reasoning");
        assert_eq!(value["max_rounds"], 10);
    }

    #[test]
    fn test_fix_truncated_json() {
        let gen = SimulationConfigGenerator::new(LLMClient::new("key", "http://x", "m"));
        let fixed = gen.fix_truncated_json(r#"{"a": [1, 2"#);
        let parsed: Value = serde_json::from_str(&fixed).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_generate_agent_config_by_rule() {
        let gen = SimulationConfigGenerator::new(LLMClient::new("key", "http://x", "m"));

        let cfg = gen.generate_agent_config_by_rule("Student");
        assert_eq!(cfg["activity_level"], 0.8);
        assert_eq!(cfg["influence_weight"], 0.8);

        let cfg = gen.generate_agent_config_by_rule("University");
        assert_eq!(cfg["activity_level"], 0.2);
        assert_eq!(cfg["influence_weight"], 3.0);

        let cfg = gen.generate_agent_config_by_rule("MediaOutlet");
        assert_eq!(cfg["activity_level"], 0.5);
        assert_eq!(cfg["stance"], "observer");
    }
}
