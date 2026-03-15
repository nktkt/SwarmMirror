/// OASIS Agent Profile Generator
///
/// Converts Zep graph entities into OASIS simulation platform Agent Profile format.
///
/// Features:
/// 1. Zep graph retrieval for enriched context
/// 2. Detailed persona generation with personality traits, demographics, MBTI
/// 3. Distinguishes between individual entities and organization/group entities
/// 4. Parallel profile generation with progress callbacks
/// 5. Realtime output saving (JSON for Reddit, CSV for Twitter)
use crate::services::llm_client::{ChatMessage, LLMClient};
use crate::services::zep_client::ZepClient;
use crate::services::zep_entity_reader::EntityNode;
use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// MBTI personality types
const MBTI_TYPES: &[&str] = &[
    "INTJ", "INTP", "ENTJ", "ENTP", "INFJ", "INFP", "ENFJ", "ENFP", "ISTJ", "ISFJ", "ESTJ",
    "ESFJ", "ISTP", "ISFP", "ESTP", "ESFP",
];

/// Common countries for fallback profile generation
const COUNTRIES: &[&str] = &[
    "China",
    "US",
    "UK",
    "Japan",
    "Germany",
    "France",
    "Canada",
    "Australia",
    "Brazil",
    "India",
    "South Korea",
];

/// Entity types that represent individual persons
const INDIVIDUAL_ENTITY_TYPES: &[&str] = &[
    "student",
    "alumni",
    "professor",
    "person",
    "publicfigure",
    "expert",
    "faculty",
    "official",
    "journalist",
    "activist",
];

/// Entity types that represent groups / organizations / institutions
const GROUP_ENTITY_TYPES: &[&str] = &[
    "university",
    "governmentagency",
    "organization",
    "ngo",
    "mediaoutlet",
    "company",
    "institution",
    "group",
    "community",
];

// ---------------------------------------------------------------------------
// OasisAgentProfile
// ---------------------------------------------------------------------------

/// Full agent profile data structure matching the Python `OasisAgentProfile`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    // Common fields
    pub user_id: usize,
    pub user_name: String,
    pub name: String,
    pub bio: String,
    pub persona: String,

    // Reddit-style optional fields
    pub karma: i32,

    // Twitter-style optional fields
    pub friend_count: i32,
    pub follower_count: i32,
    pub statuses_count: i32,

    // Extended persona information
    pub age: Option<i32>,
    pub gender: Option<String>,
    pub mbti: Option<String>,
    pub country: Option<String>,
    pub profession: Option<String>,
    pub interested_topics: Vec<String>,

    // Source entity information
    pub source_entity_uuid: Option<String>,
    pub source_entity_type: Option<String>,

    pub created_at: String,
}

impl AgentProfile {
    /// Convert to Reddit platform format (JSON object).
    ///
    /// Matches the OASIS Reddit profile schema:
    /// user_id, username, name, bio, persona, karma, created_at, plus optional extras.
    pub fn to_reddit_format(&self) -> Value {
        let mut profile = json!({
            "user_id": self.user_id,
            "username": self.user_name,
            "name": self.name,
            "bio": self.bio,
            "persona": self.persona,
            "karma": self.karma,
            "created_at": self.created_at,
        });

        let obj = profile.as_object_mut().unwrap();
        if let Some(age) = self.age {
            obj.insert("age".into(), json!(age));
        }
        if let Some(ref gender) = self.gender {
            obj.insert("gender".into(), json!(gender));
        }
        if let Some(ref mbti) = self.mbti {
            obj.insert("mbti".into(), json!(mbti));
        }
        if let Some(ref country) = self.country {
            obj.insert("country".into(), json!(country));
        }
        if let Some(ref profession) = self.profession {
            obj.insert("profession".into(), json!(profession));
        }
        if !self.interested_topics.is_empty() {
            obj.insert("interested_topics".into(), json!(self.interested_topics));
        }

        profile
    }

    /// Convert to Twitter platform format (JSON object).
    ///
    /// Matches the OASIS Twitter profile schema:
    /// user_id, username, name, bio, persona, friend_count, follower_count, statuses_count,
    /// created_at, plus optional extras.
    pub fn to_twitter_format(&self) -> Value {
        let mut profile = json!({
            "user_id": self.user_id,
            "username": self.user_name,
            "name": self.name,
            "bio": self.bio,
            "persona": self.persona,
            "friend_count": self.friend_count,
            "follower_count": self.follower_count,
            "statuses_count": self.statuses_count,
            "created_at": self.created_at,
        });

        let obj = profile.as_object_mut().unwrap();
        if let Some(age) = self.age {
            obj.insert("age".into(), json!(age));
        }
        if let Some(ref gender) = self.gender {
            obj.insert("gender".into(), json!(gender));
        }
        if let Some(ref mbti) = self.mbti {
            obj.insert("mbti".into(), json!(mbti));
        }
        if let Some(ref country) = self.country {
            obj.insert("country".into(), json!(country));
        }
        if let Some(ref profession) = self.profession {
            obj.insert("profession".into(), json!(profession));
        }
        if !self.interested_topics.is_empty() {
            obj.insert("interested_topics".into(), json!(self.interested_topics));
        }

        profile
    }

    /// Convert to a full dictionary containing every field.
    pub fn to_dict(&self) -> Value {
        json!({
            "user_id": self.user_id,
            "user_name": self.user_name,
            "name": self.name,
            "bio": self.bio,
            "persona": self.persona,
            "karma": self.karma,
            "friend_count": self.friend_count,
            "follower_count": self.follower_count,
            "statuses_count": self.statuses_count,
            "age": self.age,
            "gender": self.gender,
            "mbti": self.mbti,
            "country": self.country,
            "profession": self.profession,
            "interested_topics": self.interested_topics,
            "source_entity_uuid": self.source_entity_uuid,
            "source_entity_type": self.source_entity_type,
            "created_at": self.created_at,
        })
    }
}

// ---------------------------------------------------------------------------
// Zep search results
// ---------------------------------------------------------------------------

/// Container for Zep hybrid search results (edges + nodes).
#[derive(Debug, Default)]
struct ZepSearchResults {
    facts: Vec<String>,
    node_summaries: Vec<String>,
    context: String,
}

// ---------------------------------------------------------------------------
// ProfileGenerator
// ---------------------------------------------------------------------------

pub struct ProfileGenerator {
    llm: LLMClient,
    zep_client: Option<ZepClient>,
    graph_id: Option<String>,
}

impl ProfileGenerator {
    /// Create a new `ProfileGenerator`.
    ///
    /// * `llm` - LLM client for detailed persona generation.
    /// * `zep_api_key` - Optional Zep API key; enables context enrichment.
    /// * `graph_id` - Optional graph ID for Zep retrieval.
    pub fn new(llm: LLMClient, zep_api_key: Option<&str>, graph_id: Option<String>) -> Self {
        let zep_client = zep_api_key.map(ZepClient::new);
        Self {
            llm,
            zep_client,
            graph_id,
        }
    }

    /// Update the graph_id used for Zep retrieval.
    pub fn set_graph_id(&mut self, graph_id: String) {
        self.graph_id = Some(graph_id);
    }

    // -----------------------------------------------------------------------
    // Public entry points
    // -----------------------------------------------------------------------

    /// Generate profiles for a list of entities.
    ///
    /// This is a simpler entry point that processes entities sequentially.
    /// For parallel processing with progress callbacks and realtime saving,
    /// use [`generate_profiles_from_entities`].
    pub async fn generate_profiles(
        &self,
        entities: &[EntityNode],
        use_llm: bool,
    ) -> anyhow::Result<Vec<AgentProfile>> {
        let mut profiles = Vec::with_capacity(entities.len());
        for (i, entity) in entities.iter().enumerate() {
            let profile = if use_llm {
                self.generate_single_profile_with_llm(i, entity)
                    .await
                    .unwrap_or_else(|_| self.generate_basic_profile(i, entity))
            } else {
                self.generate_basic_profile(i, entity)
            };
            profiles.push(profile);
        }
        Ok(profiles)
    }

    /// Batch generate Agent Profiles from entities with parallel execution,
    /// progress callbacks, and realtime output saving.
    ///
    /// This mirrors the Python `generate_profiles_from_entities` method.
    ///
    /// * `entities` - Entity list to generate profiles for.
    /// * `use_llm` - Whether to use LLM for detailed persona generation.
    /// * `progress_callback` - Optional callback `(current, total, message)`.
    /// * `graph_id` - Optional graph ID for Zep retrieval.
    /// * `parallel_count` - Number of concurrent generation tasks (default 5).
    /// * `realtime_output_path` - If provided, save profiles to this path after each generation.
    /// * `output_platform` - Output platform format: `"reddit"` or `"twitter"`.
    pub async fn generate_profiles_from_entities<F>(
        &self,
        entities: &[EntityNode],
        use_llm: bool,
        progress_callback: Option<F>,
        graph_id: Option<String>,
        parallel_count: usize,
        realtime_output_path: Option<&str>,
        output_platform: &str,
    ) -> Vec<AgentProfile>
    where
        F: Fn(usize, usize, &str) + Send + Sync + 'static,
    {
        let total = entities.len();
        if total == 0 {
            return vec![];
        }

        // Apply graph_id if provided (we cannot mutate self because of shared ref,
        // so we shadow with a local graph_id for Zep lookups done via `self`).
        let _graph_id = graph_id.or_else(|| self.graph_id.clone());

        tracing::info!(
            "Starting parallel profile generation: {} entities, parallelism={}",
            total,
            parallel_count
        );

        // Shared state for collecting results and tracking progress
        let profiles: Arc<Mutex<Vec<Option<AgentProfile>>>> =
            Arc::new(Mutex::new(vec![None; total]));
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let callback = progress_callback.map(Arc::new);
        let rt_path = realtime_output_path.map(|s| s.to_string());
        let platform = output_platform.to_string();

        // Use a semaphore to limit parallelism
        let semaphore = Arc::new(tokio::sync::Semaphore::new(parallel_count));
        let mut handles = Vec::with_capacity(total);

        for (idx, entity) in entities.iter().enumerate() {
            let sem = semaphore.clone();
            let profiles_ref = profiles.clone();
            let completed_ref = completed.clone();
            let cb = callback.clone();
            let rt_path = rt_path.clone();
            let platform = platform.clone();
            let entity = entity.clone();

            // We need to share `self` across tasks. Since LLMClient and ZepClient
            // use reqwest::Client (which is internally Arc'd), we can clone the
            // relevant parts.
            let llm = self.llm.clone();
            let zep_client = self.zep_client.clone();
            let graph_id = _graph_id.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();

                let entity_type = entity.get_entity_type().unwrap_or_else(|| "Entity".to_string());

                let profile = if use_llm {
                    let gen = ProfileGenerator {
                        llm: llm.clone(),
                        zep_client: zep_client.clone(),
                        graph_id: graph_id.clone(),
                    };
                    match gen.generate_single_profile_with_llm(idx, &entity).await {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::warn!(
                                "LLM profile generation failed for {}: {}, using basic fallback",
                                entity.name,
                                e
                            );
                            let gen = ProfileGenerator {
                                llm,
                                zep_client,
                                graph_id,
                            };
                            gen.generate_basic_profile(idx, &entity)
                        }
                    }
                } else {
                    let gen = ProfileGenerator {
                        llm,
                        zep_client,
                        graph_id,
                    };
                    gen.generate_basic_profile(idx, &entity)
                };

                // Print generated profile to console
                print_generated_profile(&entity.name, &entity_type, &profile);

                // Store result
                {
                    let mut lock = profiles_ref.lock().await;
                    lock[idx] = Some(profile);
                }

                let current =
                    completed_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

                // Progress callback
                if let Some(ref cb) = cb {
                    let msg = format!(
                        "Completed {}/{}: {} ({})",
                        current, total, entity.name, entity_type
                    );
                    cb(current, total, &msg);
                }

                // Realtime output saving
                if let Some(ref path) = rt_path {
                    let lock = profiles_ref.lock().await;
                    let existing: Vec<&AgentProfile> =
                        lock.iter().filter_map(|p| p.as_ref()).collect();
                    if !existing.is_empty() {
                        let _ = save_profiles_realtime(&existing, path, &platform);
                    }
                }

                tracing::info!(
                    "[{}/{}] Generated profile: {} ({})",
                    current,
                    total,
                    entity.name,
                    entity_type
                );
            });

            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            let _ = handle.await;
        }

        let lock = profiles.lock().await;
        let result: Vec<AgentProfile> = lock.iter().filter_map(|p| p.clone()).collect();

        tracing::info!(
            "Profile generation complete: {} agents generated",
            result.len()
        );
        result
    }

    // -----------------------------------------------------------------------
    // Single profile generation
    // -----------------------------------------------------------------------

    /// Generate a single profile with LLM, with full context enrichment.
    ///
    /// Distinguishes between person entities and organization/group entities,
    /// uses different prompts for each type, generates detailed persona with
    /// personality traits, demographics, MBTI, profession, and interested topics.
    /// Uses Zep retrieval to enrich context before generation.
    async fn generate_single_profile_with_llm(
        &self,
        index: usize,
        entity: &EntityNode,
    ) -> anyhow::Result<AgentProfile> {
        let entity_type = entity
            .get_entity_type()
            .unwrap_or_else(|| "Entity".to_string());
        let name = &entity.name;
        let user_name = generate_username(name);

        // Build enriched context (including Zep retrieval)
        let context = self.build_entity_context(entity).await;

        // Generate profile data via LLM
        let profile_data = self
            .generate_profile_with_llm(
                name,
                &entity_type,
                &entity.summary,
                &entity.attributes,
                &context,
            )
            .await;

        let mut rng = rand::rng();

        Ok(AgentProfile {
            user_id: index,
            user_name,
            name: name.clone(),
            bio: profile_data
                .get("bio")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}: {}", entity_type, name)),
            persona: profile_data
                .get("persona")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    if entity.summary.is_empty() {
                        format!("A {} named {}.", entity_type, name)
                    } else {
                        entity.summary.clone()
                    }
                }),
            karma: profile_data
                .get("karma")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| rng.random_range(500..=5000)),
            friend_count: profile_data
                .get("friend_count")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| rng.random_range(50..=500)),
            follower_count: profile_data
                .get("follower_count")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| rng.random_range(100..=1000)),
            statuses_count: profile_data
                .get("statuses_count")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| rng.random_range(100..=2000)),
            age: profile_data
                .get("age")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32),
            gender: profile_data
                .get("gender")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            mbti: profile_data
                .get("mbti")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            country: profile_data
                .get("country")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            profession: profile_data
                .get("profession")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            interested_topics: profile_data
                .get("interested_topics")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            source_entity_uuid: Some(entity.uuid.clone()),
            source_entity_type: Some(entity_type),
            created_at: Utc::now().format("%Y-%m-%d").to_string(),
        })
    }

    /// Generate a basic profile without LLM (rule-based fallback).
    fn generate_basic_profile(&self, index: usize, entity: &EntityNode) -> AgentProfile {
        let entity_type = entity
            .get_entity_type()
            .unwrap_or_else(|| "Person".to_string());
        let profile_data = generate_profile_rule_based(
            &entity.name,
            &entity_type,
            &entity.summary,
            &entity.attributes,
        );

        let mut rng = rand::rng();

        AgentProfile {
            user_id: index,
            user_name: generate_username(&entity.name),
            name: entity.name.clone(),
            bio: profile_data
                .get("bio")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}: {}", entity_type, entity.name)),
            persona: profile_data
                .get("persona")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    format!("I am {}. {}", entity.name, entity.summary)
                }),
            karma: profile_data
                .get("karma")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| rng.random_range(500..=5000)),
            friend_count: profile_data
                .get("friend_count")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| rng.random_range(50..=500)),
            follower_count: profile_data
                .get("follower_count")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| rng.random_range(100..=1000)),
            statuses_count: profile_data
                .get("statuses_count")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(|| rng.random_range(100..=2000)),
            age: profile_data
                .get("age")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32),
            gender: profile_data
                .get("gender")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            mbti: profile_data
                .get("mbti")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            country: profile_data
                .get("country")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            profession: profile_data
                .get("profession")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| Some(entity_type.clone())),
            interested_topics: profile_data
                .get("interested_topics")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            source_entity_uuid: Some(entity.uuid.clone()),
            source_entity_type: Some(entity_type),
            created_at: Utc::now().format("%Y-%m-%d").to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Zep context enrichment
    // -----------------------------------------------------------------------

    /// Search Zep graph for additional context about the entity.
    ///
    /// Uses parallel edge and node searches with retry logic,
    /// deduplicates results, and builds a comprehensive context string.
    async fn enrich_with_zep_context(&self, entity: &EntityNode) -> ZepSearchResults {
        let zep_client = match &self.zep_client {
            Some(c) => c,
            None => return ZepSearchResults::default(),
        };
        let graph_id = match &self.graph_id {
            Some(g) => g.clone(),
            None => {
                tracing::debug!("Skipping Zep retrieval: no graph_id set");
                return ZepSearchResults::default();
            }
        };

        let entity_name = &entity.name;
        let query = format!(
            "All information, activities, events, relationships and background about {}",
            entity_name
        );

        // Search edges (facts/relationships) via Zep graph search
        let facts = match zep_client.search_graph(&graph_id, &query, 30).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Zep edge search failed for {}: {}", entity_name, e);
                vec![]
            }
        };

        // Build results
        let unique_facts: HashSet<String> = facts.into_iter().collect();
        let facts_vec: Vec<String> = unique_facts.into_iter().collect();

        let mut context_parts = Vec::new();
        if !facts_vec.is_empty() {
            let formatted: Vec<String> = facts_vec.iter().take(20).map(|f| format!("- {}", f)).collect();
            context_parts.push(format!("Facts:\n{}", formatted.join("\n")));
        }

        let context = context_parts.join("\n\n");

        tracing::info!(
            "Zep retrieval complete: {}, got {} facts",
            entity_name,
            facts_vec.len()
        );

        ZepSearchResults {
            facts: facts_vec,
            node_summaries: vec![],
            context,
        }
    }

    /// Build the complete context for an entity, including:
    /// 1. Entity attributes
    /// 2. Related edges (facts/relationships)
    /// 3. Related nodes information
    /// 4. Zep hybrid search results
    async fn build_entity_context(&self, entity: &EntityNode) -> String {
        let mut context_parts = Vec::new();

        // 1. Entity attributes
        if let Some(attrs) = entity.attributes.as_object() {
            let attr_lines: Vec<String> = attrs
                .iter()
                .filter(|(_, v)| !v.is_null() && v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(true))
                .map(|(k, v)| {
                    format!(
                        "- {}: {}",
                        k,
                        v.as_str().unwrap_or(&v.to_string())
                    )
                })
                .collect();
            if !attr_lines.is_empty() {
                context_parts.push(format!("### Entity Attributes\n{}", attr_lines.join("\n")));
            }
        }

        // 2. Related edges (facts/relationships)
        let mut existing_facts = HashSet::new();
        if !entity.related_edges.is_empty() {
            let mut relationships = Vec::new();
            for edge in &entity.related_edges {
                let fact = edge["fact"].as_str().unwrap_or("");
                let edge_name = edge["edge_name"].as_str().unwrap_or("");
                let direction = edge["direction"].as_str().unwrap_or("");

                if !fact.is_empty() {
                    relationships.push(format!("- {}", fact));
                    existing_facts.insert(fact.to_string());
                } else if !edge_name.is_empty() {
                    if direction == "outgoing" {
                        relationships.push(format!(
                            "- {} --[{}]--> (related entity)",
                            entity.name, edge_name
                        ));
                    } else {
                        relationships.push(format!(
                            "- (related entity) --[{}]--> {}",
                            edge_name, entity.name
                        ));
                    }
                }
            }
            if !relationships.is_empty() {
                context_parts.push(format!(
                    "### Related Facts and Relationships\n{}",
                    relationships.join("\n")
                ));
            }
        }

        // 3. Related nodes information
        if !entity.related_nodes.is_empty() {
            let mut related_info = Vec::new();
            for node in &entity.related_nodes {
                let node_name = node["name"].as_str().unwrap_or("");
                let node_labels = node["labels"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .filter(|l| *l != "Entity" && *l != "Node")
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let node_summary = node["summary"].as_str().unwrap_or("");

                let label_str = if node_labels.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", node_labels.join(", "))
                };

                if !node_summary.is_empty() {
                    related_info.push(format!("- **{}**{}: {}", node_name, label_str, node_summary));
                } else {
                    related_info.push(format!("- **{}**{}", node_name, label_str));
                }
            }
            if !related_info.is_empty() {
                context_parts.push(format!(
                    "### Related Entity Information\n{}",
                    related_info.join("\n")
                ));
            }
        }

        // 4. Zep hybrid search for richer information
        let zep_results = self.enrich_with_zep_context(entity).await;

        if !zep_results.facts.is_empty() {
            // Deduplicate: exclude facts already present from entity edges
            let new_facts: Vec<&String> = zep_results
                .facts
                .iter()
                .filter(|f| !existing_facts.contains(f.as_str()))
                .collect();
            if !new_facts.is_empty() {
                let formatted: Vec<String> =
                    new_facts.iter().take(15).map(|f| format!("- {}", f)).collect();
                context_parts.push(format!(
                    "### Zep Retrieved Facts\n{}",
                    formatted.join("\n")
                ));
            }
        }

        if !zep_results.node_summaries.is_empty() {
            let formatted: Vec<String> = zep_results
                .node_summaries
                .iter()
                .take(10)
                .map(|s| format!("- {}", s))
                .collect();
            context_parts.push(format!(
                "### Zep Retrieved Related Nodes\n{}",
                formatted.join("\n")
            ));
        }

        context_parts.join("\n\n")
    }

    // -----------------------------------------------------------------------
    // LLM prompt construction & generation
    // -----------------------------------------------------------------------

    /// Use LLM to generate a detailed persona.
    ///
    /// Distinguishes between individual entities and group/organization entities,
    /// using different prompts for each. Retries up to 3 times with decreasing
    /// temperature. Falls back to rule-based generation on failure.
    async fn generate_profile_with_llm(
        &self,
        entity_name: &str,
        entity_type: &str,
        entity_summary: &str,
        entity_attributes: &Value,
        context: &str,
    ) -> Value {
        let is_individual = is_individual_entity(entity_type);

        let prompt = if is_individual {
            build_llm_prompt_for_person(
                entity_name,
                entity_type,
                entity_summary,
                entity_attributes,
                context,
            )
        } else {
            build_llm_prompt_for_organization(
                entity_name,
                entity_type,
                entity_summary,
                entity_attributes,
                context,
            )
        };

        let system_prompt = get_system_prompt();

        let max_attempts = 3;
        let mut last_error: Option<String> = None;

        for attempt in 0..max_attempts {
            let temperature = 0.7 - (attempt as f64 * 0.1);

            let messages = vec![
                ChatMessage {
                    role: "system".into(),
                    content: system_prompt.clone(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: prompt.clone(),
                },
            ];

            match self.llm.chat_json(&messages, temperature, 4096).await {
                Ok(result) => {
                    // Validate required fields, fill defaults if missing
                    let mut result = result;
                    if result.get("bio").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
                        let bio = if entity_summary.len() > 200 {
                            &entity_summary[..200]
                        } else if !entity_summary.is_empty() {
                            entity_summary
                        } else {
                            entity_name
                        };
                        result["bio"] = json!(bio);
                    }
                    if result
                        .get("persona")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .is_empty()
                    {
                        let persona = if !entity_summary.is_empty() {
                            entity_summary.to_string()
                        } else {
                            format!("{} is a {}.", entity_name, entity_type)
                        };
                        result["persona"] = json!(persona);
                    }
                    return result;
                }
                Err(e) => {
                    tracing::warn!(
                        "LLM call failed (attempt {}/{}): {}",
                        attempt + 1,
                        max_attempts,
                        e
                    );
                    last_error = Some(e.to_string());
                    // Exponential backoff
                    tokio::time::sleep(std::time::Duration::from_secs((attempt + 1) as u64)).await;
                }
            }
        }

        tracing::warn!(
            "LLM persona generation failed after {} attempts: {:?}, using rule-based fallback",
            max_attempts,
            last_error
        );
        generate_profile_rule_based(entity_name, entity_type, entity_summary, entity_attributes)
    }

    // -----------------------------------------------------------------------
    // Save methods
    // -----------------------------------------------------------------------

    /// Save profiles to file, choosing format based on platform.
    ///
    /// * `platform = "reddit"` => JSON format
    /// * `platform = "twitter"` => CSV format
    pub fn save_profiles(
        profiles: &[AgentProfile],
        file_path: &Path,
        platform: &str,
    ) -> anyhow::Result<()> {
        if platform == "twitter" {
            Self::save_profiles_csv(profiles, file_path)
        } else {
            Self::save_profiles_json(profiles, file_path)
        }
    }

    /// Save profiles in Reddit JSON format.
    ///
    /// Produces a JSON array with OASIS-required fields including user_id, username,
    /// name, bio, persona, karma, created_at, age, gender, mbti, country.
    pub fn save_profiles_json(
        profiles: &[AgentProfile],
        path: &Path,
    ) -> anyhow::Result<()> {
        let data: Vec<Value> = profiles
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let mut item = json!({
                    "user_id": if p.user_id > 0 { p.user_id } else { idx },
                    "username": p.user_name,
                    "name": p.name,
                    "bio": truncate_str(&p.bio, 150),
                    "persona": if !p.persona.is_empty() {
                        p.persona.clone()
                    } else {
                        format!("{} is a participant in social discussions.", p.name)
                    },
                    "karma": if p.karma > 0 { p.karma } else { 1000 },
                    "created_at": p.created_at,
                    // OASIS required fields with defaults
                    "age": p.age.unwrap_or(30),
                    "gender": normalize_gender(p.gender.as_deref()),
                    "mbti": p.mbti.as_deref().unwrap_or("ISTJ"),
                    "country": p.country.as_deref().unwrap_or("中国"),
                });

                let obj = item.as_object_mut().unwrap();
                if let Some(ref profession) = p.profession {
                    obj.insert("profession".into(), json!(profession));
                }
                if !p.interested_topics.is_empty() {
                    obj.insert("interested_topics".into(), json!(p.interested_topics));
                }

                item
            })
            .collect();

        let json_str = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json_str)?;
        tracing::info!(
            "Saved {} Reddit profiles to {} (JSON format with user_id)",
            profiles.len(),
            path.display()
        );
        Ok(())
    }

    /// Save profiles in Twitter CSV format (OASIS official format).
    ///
    /// OASIS Twitter CSV fields:
    /// - user_id: sequential from 0
    /// - name: real name
    /// - username: system username
    /// - user_char: detailed persona (injected into LLM system prompt)
    /// - description: short public bio (displayed on profile page)
    pub fn save_profiles_csv(
        profiles: &[AgentProfile],
        path: &Path,
    ) -> anyhow::Result<()> {
        let path = if !path.to_string_lossy().ends_with(".csv") {
            let p = path.to_string_lossy().replace(".json", ".csv");
            std::path::PathBuf::from(if p.ends_with(".csv") { p } else { format!("{}.csv", p.trim_end_matches('.')) })
        } else {
            path.to_path_buf()
        };

        let mut lines = Vec::with_capacity(profiles.len() + 1);
        // Header
        lines.push("user_id,name,username,user_char,description".to_string());

        for (idx, profile) in profiles.iter().enumerate() {
            // user_char: full persona (bio + persona), for LLM system prompt
            let mut user_char = profile.bio.clone();
            if !profile.persona.is_empty() && profile.persona != profile.bio {
                user_char = format!("{} {}", profile.bio, profile.persona);
            }
            // Replace newlines (CSV uses spaces instead)
            let user_char = user_char.replace('\n', " ").replace('\r', " ");
            let description = profile.bio.replace('\n', " ").replace('\r', " ");

            lines.push(format!(
                "{},\"{}\",\"{}\",\"{}\",\"{}\"",
                idx,
                csv_escape(&profile.name),
                csv_escape(&profile.user_name),
                csv_escape(&user_char),
                csv_escape(&description),
            ));
        }

        std::fs::write(&path, lines.join("\n"))?;
        tracing::info!(
            "Saved {} Twitter profiles to {} (OASIS CSV format)",
            profiles.len(),
            path.display()
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Free functions (helpers)
// ---------------------------------------------------------------------------

/// Check if entity type is an individual (person) type.
fn is_individual_entity(entity_type: &str) -> bool {
    INDIVIDUAL_ENTITY_TYPES.contains(&entity_type.to_lowercase().as_str())
}

/// Check if entity type is a group/organization type.
#[allow(dead_code)]
fn is_group_entity(entity_type: &str) -> bool {
    GROUP_ENTITY_TYPES.contains(&entity_type.to_lowercase().as_str())
}

/// Generate a realistic username from an entity name.
///
/// Removes special characters, converts to lowercase, and adds a random numeric suffix.
fn generate_username(name: &str) -> String {
    let base: String = name
        .to_lowercase()
        .replace(' ', "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    let suffix = rand::rng().random_range(100..=999);
    format!("{}_{}", base, suffix)
}

/// Get the system prompt for LLM persona generation.
fn get_system_prompt() -> String {
    "你是社交媒体用户画像生成专家。生成详细、真实的人设用于舆论模拟,最大程度还原已有现实情况。\
     必须返回有效的JSON格式，所有字符串值不能包含未转义的换行符。使用中文。"
        .to_string()
}

/// Build the LLM prompt for person/individual entities.
///
/// Asks for detailed persona including:
/// - Basic demographics (age, gender, MBTI, country, profession)
/// - Background (experiences, event connections, social relationships)
/// - Personality traits (MBTI, core personality, emotional expression)
/// - Social media behavior (posting frequency, content preferences, interaction style)
/// - Stance/opinions (attitudes toward topics, triggers)
/// - Unique traits (catchphrases, special experiences, hobbies)
/// - Personal memories (connection to events, actions and reactions)
/// - Interested topics (5-10 topics)
fn build_llm_prompt_for_person(
    entity_name: &str,
    entity_type: &str,
    entity_summary: &str,
    entity_attributes: &Value,
    context: &str,
) -> String {
    let attrs_str = if entity_attributes.is_null() || entity_attributes.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        "None".to_string()
    } else {
        serde_json::to_string(entity_attributes).unwrap_or_else(|_| "None".to_string())
    };

    let context_str = if context.is_empty() {
        "No additional context"
    } else if context.len() > 3000 {
        &context[..3000]
    } else {
        context
    };

    format!(
        r#"为实体生成详细的社交媒体用户人设,最大程度还原已有现实情况。

实体名称: {entity_name}
实体类型: {entity_type}
实体摘要: {entity_summary}
实体属性: {attrs_str}

上下文信息:
{context_str}

请生成JSON，包含以下字段:

1. bio: 社交媒体简介，200字
2. persona: 详细人设描述（2000字的纯文本），需包含:
   - 基本信息（年龄、职业、教育背景、所在地）
   - 人物背景（重要经历、与事件的关联、社会关系）
   - 性格特征（MBTI类型、核心性格、情绪表达方式）
   - 社交媒体行为（发帖频率、内容偏好、互动风格、语言特点）
   - 立场观点（对话题的态度、可能被激怒/感动的内容）
   - 独特特征（口头禅、特殊经历、个人爱好）
   - 个人记忆（人设的重要部分，要介绍这个个体与事件的关联，以及这个个体在事件中的已有动作与反应）
3. age: 年龄数字（必须是整数）
4. gender: 性别，必须是英文: "male" 或 "female"
5. mbti: MBTI类型（如INTJ、ENFP等）
6. country: 国家（使用中文，如"中国"）
7. profession: 职业
8. interested_topics: 感兴趣话题数组

重要:
- 所有字段值必须是字符串或数字，不要使用换行符
- persona必须是一段连贯的文字描述
- 使用中文（除了gender字段必须用英文male/female）
- 内容要与实体信息保持一致
- age必须是有效的整数，gender必须是"male"或"female"
"#
    )
}

/// Build the LLM prompt for organization/group entities.
///
/// Generates an official account persona including:
/// - Organization info (formal name, nature, background, functions)
/// - Account positioning (type, target audience, core features)
/// - Speaking style (language traits, common expressions, taboo topics)
/// - Content characteristics (content types, posting frequency, active hours)
/// - Stance (official positions, controversy handling)
/// - Special notes (represented group profile, operational habits)
/// - Institutional memories (connection to events, actions and reactions)
fn build_llm_prompt_for_organization(
    entity_name: &str,
    entity_type: &str,
    entity_summary: &str,
    entity_attributes: &Value,
    context: &str,
) -> String {
    let attrs_str = if entity_attributes.is_null() || entity_attributes.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        "None".to_string()
    } else {
        serde_json::to_string(entity_attributes).unwrap_or_else(|_| "None".to_string())
    };

    let context_str = if context.is_empty() {
        "No additional context"
    } else if context.len() > 3000 {
        &context[..3000]
    } else {
        context
    };

    format!(
        r#"为机构/群体实体生成详细的社交媒体账号设定,最大程度还原已有现实情况。

实体名称: {entity_name}
实体类型: {entity_type}
实体摘要: {entity_summary}
实体属性: {attrs_str}

上下文信息:
{context_str}

请生成JSON，包含以下字段:

1. bio: 官方账号简介，200字，专业得体
2. persona: 详细账号设定描述（2000字的纯文本），需包含:
   - 机构基本信息（正式名称、机构性质、成立背景、主要职能）
   - 账号定位（账号类型、目标受众、核心功能）
   - 发言风格（语言特点、常用表达、禁忌话题）
   - 发布内容特点（内容类型、发布频率、活跃时间段）
   - 立场态度（对核心话题的官方立场、面对争议的处理方式）
   - 特殊说明（代表的群体画像、运营习惯）
   - 机构记忆（机构人设的重要部分，要介绍这个机构与事件的关联，以及这个机构在事件中的已有动作与反应）
3. age: 固定填30（机构账号的虚拟年龄）
4. gender: 固定填"other"（机构账号使用other表示非个人）
5. mbti: MBTI类型，用于描述账号风格，如ISTJ代表严谨保守
6. country: 国家（使用中文，如"中国"）
7. profession: 机构职能描述
8. interested_topics: 关注领域数组

重要:
- 所有字段值必须是字符串或数字，不允许null值
- persona必须是一段连贯的文字描述，不要使用换行符
- 使用中文（除了gender字段必须用英文"other"）
- age必须是整数30，gender必须是字符串"other"
- 机构账号发言要符合其身份定位
"#
    )
}

/// Generate a rule-based profile (fallback when LLM is unavailable).
///
/// Different rules for different entity types:
/// - Student/alumni: young, academic interests
/// - PublicFigure/expert/faculty: experienced, analytical
/// - MediaOutlet: institutional, news-focused
/// - University/government/NGO/organization: institutional
/// - Default: general participant
fn generate_profile_rule_based(
    entity_name: &str,
    entity_type: &str,
    entity_summary: &str,
    entity_attributes: &Value,
) -> Value {
    let entity_type_lower = entity_type.to_lowercase();
    let mut rng = rand::rng();

    fn pick_mbti(rng: &mut impl rand::Rng) -> &'static str {
        MBTI_TYPES[rng.random_range(0..MBTI_TYPES.len())]
    }
    fn pick_country(rng: &mut impl rand::Rng) -> &'static str {
        COUNTRIES[rng.random_range(0..COUNTRIES.len())]
    }
    fn pick_gender(rng: &mut impl rand::Rng) -> &'static str {
        if rng.random_bool(0.5) { "male" } else { "female" }
    }

    match entity_type_lower.as_str() {
        "student" | "alumni" => json!({
            "bio": format!("{} with interests in academics and social issues.", entity_type),
            "persona": format!(
                "{} is a {} who is actively engaged in academic and social discussions. \
                 They enjoy sharing perspectives and connecting with peers.",
                entity_name, entity_type_lower
            ),
            "age": rng.random_range(18..=30),
            "gender": pick_gender(&mut rng),
            "mbti": pick_mbti(&mut rng),
            "country": pick_country(&mut rng),
            "profession": "Student",
            "interested_topics": ["Education", "Social Issues", "Technology"],
        }),

        "publicfigure" | "expert" | "faculty" => {
            let analytical_mbtis = ["ENTJ", "INTJ", "ENTP", "INTP"];
            let mbti = analytical_mbtis[rng.random_range(0..analytical_mbtis.len())];
            let occupation = entity_attributes
                .get("occupation")
                .and_then(|v| v.as_str())
                .unwrap_or("Expert");

            json!({
                "bio": "Expert and thought leader in their field.",
                "persona": format!(
                    "{} is a recognized {} who shares insights and opinions on important matters. \
                     They are known for their expertise and influence in public discourse.",
                    entity_name, entity_type_lower
                ),
                "age": rng.random_range(35..=60),
                "gender": pick_gender(&mut rng),
                "mbti": mbti,
                "country": pick_country(&mut rng),
                "profession": occupation,
                "interested_topics": ["Politics", "Economics", "Culture & Society"],
            })
        }

        "mediaoutlet" | "socialmediaplatform" => json!({
            "bio": format!("Official account for {}. News and updates.", entity_name),
            "persona": format!(
                "{} is a media entity that reports news and facilitates public discourse. \
                 The account shares timely updates and engages with the audience on current events.",
                entity_name
            ),
            "age": 30,
            "gender": "other",
            "mbti": "ISTJ",
            "country": "中国",
            "profession": "Media",
            "interested_topics": ["General News", "Current Events", "Public Affairs"],
        }),

        "university" | "governmentagency" | "ngo" | "organization" => json!({
            "bio": format!("Official account of {}.", entity_name),
            "persona": format!(
                "{} is an institutional entity that communicates official positions, \
                 announcements, and engages with stakeholders on relevant matters.",
                entity_name
            ),
            "age": 30,
            "gender": "other",
            "mbti": "ISTJ",
            "country": "中国",
            "profession": entity_type,
            "interested_topics": ["Public Policy", "Community", "Official Announcements"],
        }),

        _ => {
            let bio = if !entity_summary.is_empty() {
                truncate_str(entity_summary, 150).to_string()
            } else {
                format!("{}: {}", entity_type, entity_name)
            };
            let persona = if !entity_summary.is_empty() {
                entity_summary.to_string()
            } else {
                format!(
                    "{} is a {} participating in social discussions.",
                    entity_name, entity_type_lower
                )
            };

            json!({
                "bio": bio,
                "persona": persona,
                "age": rng.random_range(25..=50),
                "gender": pick_gender(&mut rng),
                "mbti": pick_mbti(&mut rng),
                "country": pick_country(&mut rng),
                "profession": entity_type,
                "interested_topics": ["General", "Social Issues"],
            })
        }
    }
}

/// Print a generated profile to the console (non-truncated).
fn print_generated_profile(entity_name: &str, entity_type: &str, profile: &AgentProfile) {
    let separator = "-".repeat(70);
    let topics_str = if profile.interested_topics.is_empty() {
        "None".to_string()
    } else {
        profile.interested_topics.join(", ")
    };

    println!("\n{}", separator);
    println!("[Generated] {} ({})", entity_name, entity_type);
    println!("{}", separator);
    println!("Username: {}", profile.user_name);
    println!();
    println!("[Bio]");
    println!("{}", profile.bio);
    println!();
    println!("[Detailed Persona]");
    println!("{}", profile.persona);
    println!();
    println!("[Attributes]");
    println!(
        "Age: {:?} | Gender: {:?} | MBTI: {:?}",
        profile.age, profile.gender, profile.mbti
    );
    println!(
        "Profession: {:?} | Country: {:?}",
        profile.profession, profile.country
    );
    println!("Interested Topics: {}", topics_str);
    println!("{}", separator);
}

/// Save profiles in realtime (called after each profile is generated).
fn save_profiles_realtime(
    profiles: &[&AgentProfile],
    path: &str,
    platform: &str,
) -> anyhow::Result<()> {
    if platform == "reddit" {
        let data: Vec<Value> = profiles.iter().map(|p| p.to_reddit_format()).collect();
        let json_str = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json_str)?;
    } else {
        // Twitter CSV format
        let data: Vec<Value> = profiles.iter().map(|p| p.to_twitter_format()).collect();
        if let Some(first) = data.first() {
            if let Some(obj) = first.as_object() {
                let fieldnames: Vec<&String> = obj.keys().collect();
                let mut lines = Vec::new();
                lines.push(
                    fieldnames
                        .iter()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                for item in &data {
                    let row: Vec<String> = fieldnames
                        .iter()
                        .map(|k| {
                            let v = &item[k.as_str()];
                            if let Some(s) = v.as_str() {
                                format!("\"{}\"", csv_escape(s))
                            } else {
                                v.to_string()
                            }
                        })
                        .collect();
                    lines.push(row.join(","));
                }
                std::fs::write(path, lines.join("\n"))?;
            }
        }
    }
    Ok(())
}

/// Normalize gender field to OASIS required English format: male, female, other.
fn normalize_gender(gender: Option<&str>) -> &'static str {
    match gender {
        None => "other",
        Some(g) => {
            let g_lower = g.to_lowercase();
            match g_lower.trim() {
                "male" | "男" => "male",
                "female" | "女" => "female",
                "other" | "机构" | "其他" => "other",
                _ => "other",
            }
        }
    }
}

/// Escape a string for CSV (double up any double quotes).
fn csv_escape(s: &str) -> String {
    s.replace('"', "\"\"")
}

/// Truncate a string to at most `max_len` characters.
fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a valid char boundary
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}
