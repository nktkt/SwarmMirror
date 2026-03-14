use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use chrono::Local;
use uuid::Uuid;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SimulationStatus {
    Created,
    Preparing,
    Ready,
    Running,
    Paused,
    Stopped,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationState {
    pub simulation_id: String,
    pub project_id: String,
    pub graph_id: String,
    pub enable_twitter: bool,
    pub enable_reddit: bool,
    pub status: SimulationStatus,
    pub entities_count: usize,
    pub profiles_count: usize,
    pub entity_types: Vec<String>,
    pub config_generated: bool,
    pub config_reasoning: String,
    pub current_round: usize,
    pub twitter_status: String,
    pub reddit_status: String,
    pub created_at: String,
    pub updated_at: String,
    pub error: Option<String>,
}

impl SimulationState {
    pub fn new(project_id: &str, graph_id: &str, enable_twitter: bool, enable_reddit: bool) -> Self {
        let now = Local::now().to_rfc3339();
        Self {
            simulation_id: format!("sim_{}", &Uuid::new_v4().to_string().replace('-', "")[..12]),
            project_id: project_id.to_string(),
            graph_id: graph_id.to_string(),
            enable_twitter,
            enable_reddit,
            status: SimulationStatus::Created,
            entities_count: 0,
            profiles_count: 0,
            entity_types: Vec::new(),
            config_generated: false,
            config_reasoning: String::new(),
            current_round: 0,
            twitter_status: "not_started".to_string(),
            reddit_status: "not_started".to_string(),
            created_at: now.clone(),
            updated_at: now,
            error: None,
        }
    }
}

pub struct SimulationManager {
    data_dir: PathBuf,
}

impl SimulationManager {
    pub fn new(data_dir: &Path) -> Self {
        Self { data_dir: data_dir.to_path_buf() }
    }

    fn sim_dir(&self, sim_id: &str) -> PathBuf {
        self.data_dir.join(sim_id)
    }

    fn state_path(&self, sim_id: &str) -> PathBuf {
        self.sim_dir(sim_id).join("state.json")
    }

    pub async fn create_simulation(
        &self, project_id: &str, graph_id: &str,
        enable_twitter: bool, enable_reddit: bool,
    ) -> anyhow::Result<SimulationState> {
        let state = SimulationState::new(project_id, graph_id, enable_twitter, enable_reddit);
        let dir = self.sim_dir(&state.simulation_id);
        fs::create_dir_all(&dir).await?;
        self.save_state(&state).await?;
        Ok(state)
    }

    pub async fn save_state(&self, state: &SimulationState) -> anyhow::Result<()> {
        let mut state = state.clone();
        state.updated_at = Local::now().to_rfc3339();
        let path = self.state_path(&state.simulation_id);
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir).await?;
        let json = serde_json::to_string_pretty(&state)?;
        fs::write(&path, json).await?;
        Ok(())
    }

    pub async fn get_simulation(&self, sim_id: &str) -> Option<SimulationState> {
        let path = self.state_path(sim_id);
        let data = fs::read_to_string(&path).await.ok()?;
        serde_json::from_str(&data).ok()
    }

    pub async fn list_simulations(&self, project_id: Option<&str>) -> Vec<SimulationState> {
        let mut sims = Vec::new();
        fs::create_dir_all(&self.data_dir).await.ok();
        if let Ok(mut entries) = fs::read_dir(&self.data_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || !entry.path().is_dir() { continue; }
                if let Some(state) = self.get_simulation(&name).await {
                    if let Some(pid) = project_id {
                        if state.project_id != pid { continue; }
                    }
                    sims.push(state);
                }
            }
        }
        sims.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sims
    }

    pub async fn get_profiles(&self, sim_id: &str, platform: &str) -> anyhow::Result<serde_json::Value> {
        let path = self.sim_dir(sim_id).join(format!("{}_profiles.json", platform));
        let data = fs::read_to_string(&path).await?;
        Ok(serde_json::from_str(&data)?)
    }

    pub async fn get_config(&self, sim_id: &str) -> Option<serde_json::Value> {
        let path = self.sim_dir(sim_id).join("simulation_config.json");
        let data = fs::read_to_string(&path).await.ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn sim_dir_path(&self, sim_id: &str) -> PathBuf {
        self.sim_dir(sim_id)
    }

    pub async fn get_actions(&self, sim_id: &str) -> Vec<serde_json::Value> {
        let dir = self.sim_dir(sim_id);
        let mut actions = Vec::new();
        if let Ok(mut entries) = fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("round_") && name.ends_with(".json") {
                    if let Ok(data) = fs::read_to_string(entry.path()).await {
                        if let Ok(round_actions) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                            actions.extend(round_actions);
                        }
                    }
                }
            }
        }
        actions.sort_by_key(|a| a["round"].as_u64().unwrap_or(0));
        actions
    }

    pub async fn get_posts(&self, sim_id: &str) -> Vec<serde_json::Value> {
        self.get_actions(sim_id).await.into_iter()
            .filter(|a| {
                let t = a["action_type"].as_str().unwrap_or("");
                t == "CREATE_POST" || t == "QUOTE_POST"
            })
            .collect()
    }

    pub async fn get_comments(&self, sim_id: &str) -> Vec<serde_json::Value> {
        self.get_actions(sim_id).await.into_iter()
            .filter(|a| a["action_type"].as_str() == Some("CREATE_COMMENT"))
            .collect()
    }

    pub async fn get_timeline(&self, sim_id: &str) -> Vec<serde_json::Value> {
        let actions = self.get_actions(sim_id).await;
        let mut timeline = Vec::new();
        let mut current_round = 0u64;
        let mut round_actions = Vec::new();
        for action in &actions {
            let round = action["round"].as_u64().unwrap_or(0);
            if round != current_round && !round_actions.is_empty() {
                timeline.push(serde_json::json!({
                    "round": current_round,
                    "action_count": round_actions.len(),
                    "actions": round_actions
                }));
                round_actions = Vec::new();
            }
            current_round = round;
            round_actions.push(action.clone());
        }
        if !round_actions.is_empty() {
            timeline.push(serde_json::json!({
                "round": current_round,
                "action_count": round_actions.len(),
                "actions": round_actions
            }));
        }
        timeline
    }

    pub async fn get_agent_stats(&self, sim_id: &str) -> serde_json::Value {
        let actions = self.get_actions(sim_id).await;
        let mut stats: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
        for action in &actions {
            let agent = action["agent"].as_str().unwrap_or("unknown").to_string();
            let entry = stats.entry(agent.clone()).or_insert_with(|| serde_json::json!({
                "agent": agent,
                "total_actions": 0,
                "posts": 0,
                "comments": 0,
                "likes": 0,
                "other": 0
            }));
            if let Some(obj) = entry.as_object_mut() {
                let total = obj["total_actions"].as_u64().unwrap_or(0);
                obj.insert("total_actions".into(), serde_json::json!(total + 1));
                let action_type = action["action_type"].as_str().unwrap_or("");
                match action_type {
                    "CREATE_POST" | "QUOTE_POST" => {
                        let v = obj["posts"].as_u64().unwrap_or(0);
                        obj.insert("posts".into(), serde_json::json!(v + 1));
                    }
                    "CREATE_COMMENT" => {
                        let v = obj["comments"].as_u64().unwrap_or(0);
                        obj.insert("comments".into(), serde_json::json!(v + 1));
                    }
                    "LIKE_POST" | "LIKE_COMMENT" => {
                        let v = obj["likes"].as_u64().unwrap_or(0);
                        obj.insert("likes".into(), serde_json::json!(v + 1));
                    }
                    _ => {
                        let v = obj["other"].as_u64().unwrap_or(0);
                        obj.insert("other".into(), serde_json::json!(v + 1));
                    }
                }
            }
        }
        let agents: Vec<serde_json::Value> = stats.into_values().collect();
        serde_json::json!({
            "simulation_id": sim_id,
            "agent_count": agents.len(),
            "agents": agents,
            "total_actions": actions.len()
        })
    }
}
