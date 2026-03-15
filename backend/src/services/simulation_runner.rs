//! Simulation Runner — Native Rust implementation.
//!
//! Manages the lifecycle of social-media simulations. Each simulation spawns a
//! background `tokio` task that iterates through rounds, generates per-agent
//! actions via the LLM, maintains platform state, records all actions, and
//! saves per-round JSON logs.
//!
//! Ported from the original 1,763-line Python `simulation_runner.py` which
//! used OASIS subprocess management. Since the Python OASIS library is not
//! available in Rust, this module implements a **native simulation engine**
//! that loads agent profiles, calls the LLM for each agent's decision, and
//! maintains platform state (posts, comments, likes, follows) in memory.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::fs;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use super::llm_client::{ChatMessage, LLMClient};
use super::zep_graph_memory_updater::ZepGraphMemoryManager;

// ============================================================================
// RunnerStatus
// ============================================================================

/// Current status of a simulation runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStatus {
    /// Not yet started (initial state).
    Idle,
    /// Initialising — loading config, profiles, etc.
    Starting,
    /// Actively running rounds.
    Running,
    /// Paused by user request.
    Paused,
    /// A stop has been requested; waiting for graceful shutdown.
    Stopping,
    /// Stopped by user.
    Stopped,
    /// All rounds completed successfully.
    Completed,
    /// An error occurred.
    Failed,
}

impl std::fmt::Display for RunnerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerStatus::Idle => write!(f, "idle"),
            RunnerStatus::Starting => write!(f, "starting"),
            RunnerStatus::Running => write!(f, "running"),
            RunnerStatus::Paused => write!(f, "paused"),
            RunnerStatus::Stopping => write!(f, "stopping"),
            RunnerStatus::Stopped => write!(f, "stopped"),
            RunnerStatus::Completed => write!(f, "completed"),
            RunnerStatus::Failed => write!(f, "failed"),
        }
    }
}

// ============================================================================
// AgentAction
// ============================================================================

/// A single agent action recorded during a simulation round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    /// The round number this action occurred in.
    pub round_num: usize,
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Platform name (`twitter` / `reddit`).
    pub platform: String,
    /// Numeric agent ID.
    pub agent_id: usize,
    /// Human-readable agent name.
    pub agent_name: String,
    /// Action type (CREATE_POST, LIKE_POST, REPOST, FOLLOW, CREATE_COMMENT, etc.).
    pub action_type: String,
    /// Arbitrary action arguments (content, target, etc.).
    #[serde(default)]
    pub action_args: HashMap<String, Value>,
    /// Optional result / response text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Whether the action was successful.
    #[serde(default = "default_true")]
    pub success: bool,
}

fn default_true() -> bool {
    true
}

impl AgentAction {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(json!({}))
    }
}

// ============================================================================
// RoundSummary
// ============================================================================

/// Summary statistics for a single simulation round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundSummary {
    pub round_num: usize,
    pub start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    pub simulated_hour: usize,
    pub twitter_actions: usize,
    pub reddit_actions: usize,
    pub active_agents: Vec<usize>,
    pub actions: Vec<AgentAction>,
}

impl RoundSummary {
    pub fn to_value(&self) -> Value {
        json!({
            "round_num": self.round_num,
            "start_time": self.start_time,
            "end_time": self.end_time,
            "simulated_hour": self.simulated_hour,
            "twitter_actions": self.twitter_actions,
            "reddit_actions": self.reddit_actions,
            "active_agents": self.active_agents,
            "actions_count": self.actions.len(),
            "actions": self.actions.iter().map(|a| a.to_value()).collect::<Vec<_>>(),
        })
    }
}

// ============================================================================
// SimulationRunState
// ============================================================================

/// Live run-state of a simulation. Persisted to `run_state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationRunState {
    pub simulation_id: String,
    pub runner_status: RunnerStatus,

    // Progress
    pub current_round: usize,
    pub total_rounds: usize,
    pub simulated_hours: usize,
    pub total_simulation_hours: usize,

    // Per-platform independent round and simulated-time tracking
    pub twitter_current_round: usize,
    pub reddit_current_round: usize,
    pub twitter_simulated_hours: usize,
    pub reddit_simulated_hours: usize,

    // Platform status
    pub twitter_running: bool,
    pub reddit_running: bool,
    pub twitter_actions_count: usize,
    pub reddit_actions_count: usize,

    // Platform completion (detected from actions.jsonl simulation_end event)
    pub twitter_completed: bool,
    pub reddit_completed: bool,

    // Recent actions (most-recent first, capped at `max_recent_actions`)
    #[serde(default)]
    pub recent_actions: Vec<AgentAction>,
    #[serde(default = "default_max_recent")]
    pub max_recent_actions: usize,

    // Timestamps
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,

    // Error message (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    // Process PID (for subprocess-mode; 0 for native)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_pid: Option<u32>,
}

fn default_max_recent() -> usize {
    50
}

impl SimulationRunState {
    /// Create a new run-state in `Idle`.
    pub fn new(simulation_id: &str) -> Self {
        Self {
            simulation_id: simulation_id.to_string(),
            runner_status: RunnerStatus::Idle,
            current_round: 0,
            total_rounds: 0,
            simulated_hours: 0,
            total_simulation_hours: 0,
            twitter_current_round: 0,
            reddit_current_round: 0,
            twitter_simulated_hours: 0,
            reddit_simulated_hours: 0,
            twitter_running: false,
            reddit_running: false,
            twitter_actions_count: 0,
            reddit_actions_count: 0,
            twitter_completed: false,
            reddit_completed: false,
            recent_actions: Vec::new(),
            max_recent_actions: 50,
            started_at: None,
            updated_at: Utc::now().to_rfc3339(),
            completed_at: None,
            error: None,
            process_pid: None,
        }
    }

    /// Push an action to `recent_actions` (most-recent-first), capping length.
    pub fn add_action(&mut self, action: AgentAction) {
        if action.platform == "twitter" {
            self.twitter_actions_count += 1;
        } else {
            self.reddit_actions_count += 1;
        }
        self.recent_actions.insert(0, action);
        if self.recent_actions.len() > self.max_recent_actions {
            self.recent_actions.truncate(self.max_recent_actions);
        }
        self.updated_at = Utc::now().to_rfc3339();
    }

    /// Serialise to the shape expected by the frontend.
    pub fn to_value(&self) -> Value {
        let progress = if self.total_rounds > 0 {
            (self.current_round as f64 / self.total_rounds as f64 * 100.0 * 10.0).round() / 10.0
        } else {
            0.0
        };
        json!({
            "simulation_id": self.simulation_id,
            "runner_status": self.runner_status,
            "current_round": self.current_round,
            "total_rounds": self.total_rounds,
            "simulated_hours": self.simulated_hours,
            "total_simulation_hours": self.total_simulation_hours,
            "progress_percent": progress,
            "twitter_current_round": self.twitter_current_round,
            "reddit_current_round": self.reddit_current_round,
            "twitter_simulated_hours": self.twitter_simulated_hours,
            "reddit_simulated_hours": self.reddit_simulated_hours,
            "twitter_running": self.twitter_running,
            "reddit_running": self.reddit_running,
            "twitter_completed": self.twitter_completed,
            "reddit_completed": self.reddit_completed,
            "twitter_actions_count": self.twitter_actions_count,
            "reddit_actions_count": self.reddit_actions_count,
            "total_actions_count": self.twitter_actions_count + self.reddit_actions_count,
            "started_at": self.started_at,
            "updated_at": self.updated_at,
            "completed_at": self.completed_at,
            "error": self.error,
            "process_pid": self.process_pid,
        })
    }

    /// Detailed variant including recent actions.
    pub fn to_detail_value(&self) -> Value {
        let mut v = self.to_value();
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "recent_actions".to_string(),
                json!(self.recent_actions.iter().map(|a| a.to_value()).collect::<Vec<_>>()),
            );
        }
        v
    }
}

// ============================================================================
// Platform State — maintained in memory during a simulation run
// ============================================================================

/// A post on the simulated platform.
#[derive(Debug, Clone, Serialize)]
struct PlatformPost {
    post_id: usize,
    author_id: usize,
    author_name: String,
    content: String,
    likes: Vec<usize>,
    dislikes: Vec<usize>,
    reposts: Vec<usize>,
    comments: Vec<PlatformComment>,
    round_created: usize,
    timestamp: String,
}

/// A comment on a post.
#[derive(Debug, Clone, Serialize)]
struct PlatformComment {
    comment_id: usize,
    author_id: usize,
    author_name: String,
    content: String,
    likes: Vec<usize>,
    dislikes: Vec<usize>,
    round_created: usize,
    timestamp: String,
}

/// In-memory state for one platform (twitter or reddit).
#[derive(Debug)]
struct PlatformState {
    posts: Vec<PlatformPost>,
    /// agent_id -> list of followed agent_ids
    follows: HashMap<usize, Vec<usize>>,
    /// agent_id -> list of muted agent_ids
    mutes: HashMap<usize, Vec<usize>>,
    next_post_id: usize,
    next_comment_id: usize,
}

impl PlatformState {
    fn new() -> Self {
        Self {
            posts: Vec::new(),
            follows: HashMap::new(),
            mutes: HashMap::new(),
            next_post_id: 1,
            next_comment_id: 1,
        }
    }

    /// Build a feed summary string for the given agent (recent posts they can see).
    fn build_feed(&self, agent_id: usize, max_posts: usize) -> String {
        let muted = self.mutes.get(&agent_id).cloned().unwrap_or_default();
        let mut visible: Vec<&PlatformPost> = self
            .posts
            .iter()
            .filter(|p| !muted.contains(&p.author_id))
            .collect();
        // Most recent first
        visible.sort_by(|a, b| b.post_id.cmp(&a.post_id));
        visible.truncate(max_posts);

        if visible.is_empty() {
            return "No posts on the timeline yet.".to_string();
        }

        let mut feed = String::new();
        for p in &visible {
            let like_count = p.likes.len();
            let comment_count = p.comments.len();
            feed.push_str(&format!(
                "[Post #{}] @{}: \"{}\"\n  likes={}, comments={}, reposts={}\n",
                p.post_id,
                p.author_name,
                p.content,
                like_count,
                comment_count,
                p.reposts.len(),
            ));
            // Show up to 2 comments
            for c in p.comments.iter().rev().take(2) {
                feed.push_str(&format!(
                    "  > @{}: \"{}\"\n",
                    c.author_name, c.content
                ));
            }
        }
        feed
    }

    fn create_post(
        &mut self,
        author_id: usize,
        author_name: &str,
        content: &str,
        round: usize,
    ) -> usize {
        let id = self.next_post_id;
        self.next_post_id += 1;
        self.posts.push(PlatformPost {
            post_id: id,
            author_id,
            author_name: author_name.to_string(),
            content: content.to_string(),
            likes: Vec::new(),
            dislikes: Vec::new(),
            reposts: Vec::new(),
            comments: Vec::new(),
            round_created: round,
            timestamp: Utc::now().to_rfc3339(),
        });
        id
    }

    fn like_post(&mut self, post_id: usize, agent_id: usize) -> bool {
        if let Some(p) = self.posts.iter_mut().find(|p| p.post_id == post_id) {
            if !p.likes.contains(&agent_id) {
                p.likes.push(agent_id);
                return true;
            }
        }
        false
    }

    fn dislike_post(&mut self, post_id: usize, agent_id: usize) -> bool {
        if let Some(p) = self.posts.iter_mut().find(|p| p.post_id == post_id) {
            if !p.dislikes.contains(&agent_id) {
                p.dislikes.push(agent_id);
                return true;
            }
        }
        false
    }

    fn repost(&mut self, post_id: usize, agent_id: usize) -> bool {
        if let Some(p) = self.posts.iter_mut().find(|p| p.post_id == post_id) {
            if !p.reposts.contains(&agent_id) {
                p.reposts.push(agent_id);
                return true;
            }
        }
        false
    }

    fn add_comment(
        &mut self,
        post_id: usize,
        author_id: usize,
        author_name: &str,
        content: &str,
        round: usize,
    ) -> Option<usize> {
        if let Some(p) = self.posts.iter_mut().find(|p| p.post_id == post_id) {
            let cid = self.next_comment_id;
            self.next_comment_id += 1;
            p.comments.push(PlatformComment {
                comment_id: cid,
                author_id,
                author_name: author_name.to_string(),
                content: content.to_string(),
                likes: Vec::new(),
                dislikes: Vec::new(),
                round_created: round,
                timestamp: Utc::now().to_rfc3339(),
            });
            Some(cid)
        } else {
            None
        }
    }

    fn follow(&mut self, follower_id: usize, target_id: usize) {
        self.follows
            .entry(follower_id)
            .or_default()
            .push(target_id);
    }

    fn mute(&mut self, agent_id: usize, target_id: usize) {
        self.mutes
            .entry(agent_id)
            .or_default()
            .push(target_id);
    }

    /// Get a post by id (for building action_args).
    fn get_post(&self, post_id: usize) -> Option<&PlatformPost> {
        self.posts.iter().find(|p| p.post_id == post_id)
    }
}

// ============================================================================
// Agent profile (loaded from profiles JSON)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentProfile {
    #[serde(alias = "user_id")]
    agent_id: usize,
    #[serde(alias = "username")]
    name: String,
    #[serde(default)]
    persona: String,
    #[serde(default)]
    bio: String,
    #[serde(default)]
    interests: Vec<String>,
    #[serde(default)]
    personality_traits: Vec<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

// ============================================================================
// Global singleton maps (mirrors the Python classmethod pattern)
// ============================================================================

/// In-memory run states, keyed by simulation_id.
static RUN_STATES: Lazy<DashMap<String, SimulationRunState>> = Lazy::new(DashMap::new);
/// Active simulation task handles.
static SIM_HANDLES: Lazy<DashMap<String, JoinHandle<()>>> = Lazy::new(DashMap::new);
/// Per-simulation pause notify — tokio tasks wait on this.
static PAUSE_NOTIFIERS: Lazy<DashMap<String, Arc<Notify>>> = Lazy::new(DashMap::new);
/// Per-simulation stop flag.
static STOP_FLAGS: Lazy<DashMap<String, Arc<RwLock<bool>>>> = Lazy::new(DashMap::new);
/// Per-simulation pause flag.
static PAUSE_FLAGS: Lazy<DashMap<String, Arc<RwLock<bool>>>> = Lazy::new(DashMap::new);
/// Global cleanup flag.
static CLEANUP_DONE: Lazy<RwLock<bool>> = Lazy::new(|| RwLock::new(false));

// ============================================================================
// SimulationRunner
// ============================================================================

/// The simulation runner manages the complete lifecycle of simulations.
pub struct SimulationRunner;

impl SimulationRunner {
    // ------------------------------------------------------------------
    // State persistence
    // ------------------------------------------------------------------

    /// Load a run state from the in-memory cache, falling back to disk.
    pub async fn get_run_state(
        sim_dir: &Path,
        simulation_id: &str,
    ) -> Option<SimulationRunState> {
        // Check memory first
        if let Some(entry) = RUN_STATES.get(simulation_id) {
            return Some(entry.value().clone());
        }
        // Try loading from disk
        let state_file = sim_dir.join("run_state.json");
        Self::load_run_state_from_file(&state_file, simulation_id).await
    }

    async fn load_run_state_from_file(
        path: &Path,
        simulation_id: &str,
    ) -> Option<SimulationRunState> {
        let data = fs::read_to_string(path).await.ok()?;
        let v: Value = serde_json::from_str(&data).ok()?;

        let state = SimulationRunState {
            simulation_id: simulation_id.to_string(),
            runner_status: serde_json::from_value(
                v.get("runner_status")
                    .cloned()
                    .unwrap_or(json!("idle")),
            )
            .unwrap_or(RunnerStatus::Idle),
            current_round: v["current_round"].as_u64().unwrap_or(0) as usize,
            total_rounds: v["total_rounds"].as_u64().unwrap_or(0) as usize,
            simulated_hours: v["simulated_hours"].as_u64().unwrap_or(0) as usize,
            total_simulation_hours: v["total_simulation_hours"].as_u64().unwrap_or(0) as usize,
            twitter_current_round: v["twitter_current_round"].as_u64().unwrap_or(0) as usize,
            reddit_current_round: v["reddit_current_round"].as_u64().unwrap_or(0) as usize,
            twitter_simulated_hours: v["twitter_simulated_hours"].as_u64().unwrap_or(0) as usize,
            reddit_simulated_hours: v["reddit_simulated_hours"].as_u64().unwrap_or(0) as usize,
            twitter_running: v["twitter_running"].as_bool().unwrap_or(false),
            reddit_running: v["reddit_running"].as_bool().unwrap_or(false),
            twitter_actions_count: v["twitter_actions_count"].as_u64().unwrap_or(0) as usize,
            reddit_actions_count: v["reddit_actions_count"].as_u64().unwrap_or(0) as usize,
            twitter_completed: v["twitter_completed"].as_bool().unwrap_or(false),
            reddit_completed: v["reddit_completed"].as_bool().unwrap_or(false),
            recent_actions: v
                .get("recent_actions")
                .and_then(|ra| serde_json::from_value::<Vec<AgentAction>>(ra.clone()).ok())
                .unwrap_or_default(),
            max_recent_actions: v["max_recent_actions"].as_u64().unwrap_or(50) as usize,
            started_at: v["started_at"].as_str().map(|s| s.to_string()),
            updated_at: v["updated_at"]
                .as_str()
                .unwrap_or(&Utc::now().to_rfc3339())
                .to_string(),
            completed_at: v["completed_at"].as_str().map(|s| s.to_string()),
            error: v["error"].as_str().map(|s| s.to_string()),
            process_pid: v["process_pid"].as_u64().map(|p| p as u32),
        };

        RUN_STATES.insert(simulation_id.to_string(), state.clone());
        Some(state)
    }

    /// Save run state to memory and disk.
    pub async fn save_run_state(sim_dir: &Path, state: &SimulationRunState) -> Result<()> {
        fs::create_dir_all(sim_dir).await?;
        let state_file = sim_dir.join("run_state.json");
        let data = state.to_detail_value();
        let json_str = serde_json::to_string_pretty(&data)?;
        fs::write(&state_file, json_str).await?;
        RUN_STATES.insert(state.simulation_id.clone(), state.clone());
        Ok(())
    }

    // ------------------------------------------------------------------
    // Start / Stop / Pause / Resume
    // ------------------------------------------------------------------

    /// Start a new simulation.
    ///
    /// Loads the configuration and profiles from `sim_dir`, initialises
    /// the run state, and spawns a background task that drives the rounds.
    pub async fn start_simulation(
        sim_dir: &Path,
        simulation_id: &str,
        platform: &str,  // "twitter" / "reddit" / "parallel"
        max_rounds: Option<usize>,
        llm: LLMClient,
        graph_memory_manager: Option<Arc<Mutex<ZepGraphMemoryManager>>>,
        graph_id: Option<String>,
    ) -> Result<SimulationRunState> {
        // Check if already running
        if let Some(entry) = RUN_STATES.get(simulation_id) {
            let st = entry.runner_status.clone();
            if st == RunnerStatus::Running || st == RunnerStatus::Starting {
                bail!("Simulation is already running: {}", simulation_id);
            }
        }

        // Load config
        let config_path = sim_dir.join("simulation_config.json");
        if !config_path.exists() {
            bail!("Simulation config not found. Please call /prepare first.");
        }
        let config_raw = fs::read_to_string(&config_path).await?;
        let config: Value = serde_json::from_str(&config_raw)?;

        // Compute total rounds from time_config
        let time_config = config.get("time_config").cloned().unwrap_or(json!({}));
        let total_hours = time_config["total_simulation_hours"].as_u64().unwrap_or(72) as usize;
        let minutes_per_round = time_config["minutes_per_round"].as_u64().unwrap_or(30) as usize;
        let mut total_rounds = if minutes_per_round > 0 {
            total_hours * 60 / minutes_per_round
        } else {
            total_hours * 2
        };

        // Apply max_rounds cap
        if let Some(mr) = max_rounds {
            if mr > 0 && mr < total_rounds {
                info!(
                    "Capping rounds: {} -> {} (max_rounds={})",
                    total_rounds, mr, mr
                );
                total_rounds = mr;
            }
        }

        // Determine which platforms to enable
        let enable_twitter = platform == "twitter" || platform == "parallel";
        let enable_reddit = platform == "reddit" || platform == "parallel";

        // Initialize run state
        let mut state = SimulationRunState::new(simulation_id);
        state.runner_status = RunnerStatus::Starting;
        state.total_rounds = total_rounds;
        state.total_simulation_hours = total_hours;
        state.started_at = Some(Utc::now().to_rfc3339());
        state.twitter_running = enable_twitter;
        state.reddit_running = enable_reddit;

        Self::save_run_state(sim_dir, &state).await?;

        // Set up graph memory updater if requested
        if let (Some(ref manager), Some(ref _gid)) = (&graph_memory_manager, &graph_id) {
            let m = manager.lock().await;
            // We don't have the API key here; the manager should already be configured.
            if m.has_updater(simulation_id).await {
                info!("Graph memory updater already exists for {}", simulation_id);
            }
        }

        // Initialise control flags
        let stop_flag = Arc::new(RwLock::new(false));
        let pause_flag = Arc::new(RwLock::new(false));
        let pause_notify = Arc::new(Notify::new());

        STOP_FLAGS.insert(simulation_id.to_string(), stop_flag.clone());
        PAUSE_FLAGS.insert(simulation_id.to_string(), pause_flag.clone());
        PAUSE_NOTIFIERS.insert(simulation_id.to_string(), pause_notify.clone());

        // Spawn background simulation task
        let sim_dir_owned = sim_dir.to_path_buf();
        let sim_id_owned = simulation_id.to_string();
        let platform_owned = platform.to_string();
        let config_clone = config.clone();
        let gmm_clone = graph_memory_manager.clone();

        let handle = tokio::spawn(async move {
            let result = Self::run_simulation_loop(
                &sim_dir_owned,
                &sim_id_owned,
                &platform_owned,
                enable_twitter,
                enable_reddit,
                total_rounds,
                minutes_per_round,
                &config_clone,
                llm,
                stop_flag,
                pause_flag,
                pause_notify,
                gmm_clone,
            )
            .await;

            if let Err(e) = result {
                error!("Simulation {} failed: {}", sim_id_owned, e);
                if let Some(mut entry) = RUN_STATES.get_mut(&sim_id_owned) {
                    entry.runner_status = RunnerStatus::Failed;
                    entry.error = Some(e.to_string());
                    entry.twitter_running = false;
                    entry.reddit_running = false;
                    let _ = Self::save_run_state(&sim_dir_owned, &entry).await;
                }
            }

            // Cleanup
            STOP_FLAGS.remove(&sim_id_owned);
            PAUSE_FLAGS.remove(&sim_id_owned);
            PAUSE_NOTIFIERS.remove(&sim_id_owned);
            SIM_HANDLES.remove(&sim_id_owned);
        });

        SIM_HANDLES.insert(simulation_id.to_string(), handle);

        // Re-read state (may have been updated)
        let state = RUN_STATES
            .get(simulation_id)
            .map(|e| e.value().clone())
            .unwrap_or(state);

        info!(
            "Simulation started: {}, platform={}, total_rounds={}",
            simulation_id, platform, total_rounds
        );

        Ok(state)
    }

    /// Stop a running simulation.
    pub async fn stop_simulation(sim_dir: &Path, simulation_id: &str) -> Result<SimulationRunState> {
        // Set the stop flag
        if let Some(flag) = STOP_FLAGS.get(simulation_id) {
            let mut w = flag.write().await;
            *w = true;
        }
        // Wake up paused task if any
        if let Some(notify) = PAUSE_NOTIFIERS.get(simulation_id) {
            notify.notify_one();
        }

        // Update state
        if let Some(mut entry) = RUN_STATES.get_mut(simulation_id) {
            entry.runner_status = RunnerStatus::Stopping;
            let _ = Self::save_run_state(sim_dir, &entry).await;
        }

        // Wait briefly for the task to finish
        if let Some((_, handle)) = SIM_HANDLES.remove(simulation_id) {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(10), handle).await;
        }

        // Final state update
        if let Some(mut entry) = RUN_STATES.get_mut(simulation_id) {
            entry.runner_status = RunnerStatus::Stopped;
            entry.twitter_running = false;
            entry.reddit_running = false;
            entry.completed_at = Some(Utc::now().to_rfc3339());
            let _ = Self::save_run_state(sim_dir, &entry).await;
            info!("Simulation stopped: {}", simulation_id);
            return Ok(entry.clone());
        }

        bail!("Simulation state not found: {}", simulation_id)
    }

    /// Pause a running simulation.
    pub async fn pause_simulation(sim_dir: &Path, simulation_id: &str) -> Result<SimulationRunState> {
        if let Some(flag) = PAUSE_FLAGS.get(simulation_id) {
            let mut w = flag.write().await;
            *w = true;
        } else {
            bail!("Simulation is not running: {}", simulation_id);
        }

        if let Some(mut entry) = RUN_STATES.get_mut(simulation_id) {
            entry.runner_status = RunnerStatus::Paused;
            let _ = Self::save_run_state(sim_dir, &entry).await;
            info!("Simulation paused: {}", simulation_id);
            return Ok(entry.clone());
        }

        bail!("Simulation state not found: {}", simulation_id)
    }

    /// Resume a paused simulation.
    pub async fn resume_simulation(sim_dir: &Path, simulation_id: &str) -> Result<SimulationRunState> {
        if let Some(flag) = PAUSE_FLAGS.get(simulation_id) {
            let mut w = flag.write().await;
            *w = false;
        }
        if let Some(notify) = PAUSE_NOTIFIERS.get(simulation_id) {
            notify.notify_one();
        }

        if let Some(mut entry) = RUN_STATES.get_mut(simulation_id) {
            entry.runner_status = RunnerStatus::Running;
            let _ = Self::save_run_state(sim_dir, &entry).await;
            info!("Simulation resumed: {}", simulation_id);
            return Ok(entry.clone());
        }

        bail!("Simulation state not found: {}", simulation_id)
    }

    /// Check if a simulation is currently running.
    pub fn is_running(simulation_id: &str) -> bool {
        RUN_STATES
            .get(simulation_id)
            .map(|e| e.runner_status == RunnerStatus::Running)
            .unwrap_or(false)
    }

    /// Get a list of all currently running simulation IDs.
    pub fn get_running_simulations() -> Vec<String> {
        SIM_HANDLES
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    // ------------------------------------------------------------------
    // Query helpers
    // ------------------------------------------------------------------

    /// Read all actions from JSONL action log files (twitter/ and reddit/).
    pub async fn get_all_actions(
        sim_dir: &Path,
        platform_filter: Option<&str>,
        agent_id: Option<usize>,
        round_num: Option<usize>,
    ) -> Vec<AgentAction> {
        let mut actions = Vec::new();

        let platforms: Vec<(&str, &str)> = match platform_filter {
            Some("twitter") => vec![("twitter", "twitter")],
            Some("reddit") => vec![("reddit", "reddit")],
            _ => vec![("twitter", "twitter"), ("reddit", "reddit")],
        };

        for (dir_name, default_platform) in &platforms {
            let log_path = sim_dir.join(dir_name).join("actions.jsonl");
            if let Ok(data) = fs::read_to_string(&log_path).await {
                for line in data.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let parsed: Value = match serde_json::from_str(line) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Skip event-type entries
                    if parsed.get("event_type").is_some() {
                        continue;
                    }
                    if parsed.get("agent_id").is_none() {
                        continue;
                    }

                    let record_platform = parsed["platform"]
                        .as_str()
                        .unwrap_or(default_platform)
                        .to_string();

                    if let Some(pf) = platform_filter {
                        if record_platform != pf {
                            continue;
                        }
                    }
                    let aid = parsed["agent_id"].as_u64().unwrap_or(0) as usize;
                    if let Some(fid) = agent_id {
                        if aid != fid {
                            continue;
                        }
                    }
                    let rn = parsed["round"].as_u64().unwrap_or(0) as usize;
                    if let Some(fr) = round_num {
                        if rn != fr {
                            continue;
                        }
                    }

                    actions.push(AgentAction {
                        round_num: rn,
                        timestamp: parsed["timestamp"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        platform: record_platform,
                        agent_id: aid,
                        agent_name: parsed["agent_name"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        action_type: parsed["action_type"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                        action_args: parsed["action_args"]
                            .as_object()
                            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default(),
                        result: parsed["result"].as_str().map(|s| s.to_string()),
                        success: parsed["success"].as_bool().unwrap_or(true),
                    });
                }
            }
        }

        // Fall back to legacy single actions.jsonl
        if actions.is_empty() {
            let legacy = sim_dir.join("actions.jsonl");
            if let Ok(data) = fs::read_to_string(&legacy).await {
                for line in data.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(parsed) = serde_json::from_str::<Value>(line) {
                        if parsed.get("event_type").is_some() || parsed.get("agent_id").is_none() {
                            continue;
                        }
                        let rp = parsed["platform"].as_str().unwrap_or("").to_string();
                        if let Some(pf) = platform_filter {
                            if rp != pf {
                                continue;
                            }
                        }
                        let aid = parsed["agent_id"].as_u64().unwrap_or(0) as usize;
                        if let Some(fid) = agent_id {
                            if aid != fid {
                                continue;
                            }
                        }
                        let rn = parsed["round"].as_u64().unwrap_or(0) as usize;
                        if let Some(fr) = round_num {
                            if rn != fr {
                                continue;
                            }
                        }
                        actions.push(AgentAction {
                            round_num: rn,
                            timestamp: parsed["timestamp"].as_str().unwrap_or("").to_string(),
                            platform: rp,
                            agent_id: aid,
                            agent_name: parsed["agent_name"].as_str().unwrap_or("").to_string(),
                            action_type: parsed["action_type"].as_str().unwrap_or("").to_string(),
                            action_args: parsed["action_args"]
                                .as_object()
                                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                                .unwrap_or_default(),
                            result: parsed["result"].as_str().map(|s| s.to_string()),
                            success: parsed["success"].as_bool().unwrap_or(true),
                        });
                    }
                }
            }
        }

        // Sort by timestamp descending (newest first)
        actions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        actions
    }

    /// Get actions with pagination.
    pub async fn get_actions(
        sim_dir: &Path,
        limit: usize,
        offset: usize,
        platform: Option<&str>,
        agent_id: Option<usize>,
        round_num: Option<usize>,
    ) -> Vec<AgentAction> {
        let all = Self::get_all_actions(sim_dir, platform, agent_id, round_num).await;
        all.into_iter().skip(offset).take(limit).collect()
    }

    /// Get a timeline — actions grouped by round.
    pub async fn get_timeline(
        sim_dir: &Path,
        start_round: usize,
        end_round: Option<usize>,
    ) -> Vec<Value> {
        let actions = Self::get_all_actions(sim_dir, None, None, None).await;
        let mut rounds: HashMap<usize, Value> = HashMap::new();

        for action in &actions {
            let rn = action.round_num;
            if rn < start_round {
                continue;
            }
            if let Some(er) = end_round {
                if rn > er {
                    continue;
                }
            }

            let entry = rounds.entry(rn).or_insert_with(|| {
                json!({
                    "round_num": rn,
                    "twitter_actions": 0,
                    "reddit_actions": 0,
                    "active_agents": Vec::<usize>::new(),
                    "action_types": {},
                    "first_action_time": &action.timestamp,
                    "last_action_time": &action.timestamp,
                })
            });

            if let Some(obj) = entry.as_object_mut() {
                if action.platform == "twitter" {
                    let c = obj["twitter_actions"].as_u64().unwrap_or(0);
                    obj.insert("twitter_actions".into(), json!(c + 1));
                } else {
                    let c = obj["reddit_actions"].as_u64().unwrap_or(0);
                    obj.insert("reddit_actions".into(), json!(c + 1));
                }

                // active_agents
                if let Some(arr) = obj.get_mut("active_agents").and_then(|v| v.as_array_mut()) {
                    let aid_val = json!(action.agent_id);
                    if !arr.contains(&aid_val) {
                        arr.push(aid_val);
                    }
                }

                // action_types
                if let Some(at) = obj.get_mut("action_types").and_then(|v| v.as_object_mut()) {
                    let count = at
                        .get(&action.action_type)
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    at.insert(action.action_type.clone(), json!(count + 1));
                }

                obj.insert("last_action_time".into(), json!(&action.timestamp));
            }
        }

        let mut result: Vec<Value> = rounds.into_values().collect();
        result.sort_by_key(|v| v["round_num"].as_u64().unwrap_or(0));

        // Add total_actions and active_agents_count
        for v in &mut result {
            if let Some(obj) = v.as_object_mut() {
                let tw = obj["twitter_actions"].as_u64().unwrap_or(0);
                let rd = obj["reddit_actions"].as_u64().unwrap_or(0);
                obj.insert("total_actions".into(), json!(tw + rd));
                let ac = obj["active_agents"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0);
                obj.insert("active_agents_count".into(), json!(ac));
            }
        }

        result
    }

    /// Get per-agent statistics.
    pub async fn get_agent_stats(sim_dir: &Path) -> Vec<Value> {
        let actions = Self::get_all_actions(sim_dir, None, None, None).await;
        let mut stats: HashMap<usize, Value> = HashMap::new();

        for action in &actions {
            let entry = stats.entry(action.agent_id).or_insert_with(|| {
                json!({
                    "agent_id": action.agent_id,
                    "agent_name": &action.agent_name,
                    "total_actions": 0,
                    "twitter_actions": 0,
                    "reddit_actions": 0,
                    "action_types": {},
                    "first_action_time": &action.timestamp,
                    "last_action_time": &action.timestamp,
                })
            });

            if let Some(obj) = entry.as_object_mut() {
                let t = obj["total_actions"].as_u64().unwrap_or(0);
                obj.insert("total_actions".into(), json!(t + 1));

                if action.platform == "twitter" {
                    let c = obj["twitter_actions"].as_u64().unwrap_or(0);
                    obj.insert("twitter_actions".into(), json!(c + 1));
                } else {
                    let c = obj["reddit_actions"].as_u64().unwrap_or(0);
                    obj.insert("reddit_actions".into(), json!(c + 1));
                }

                if let Some(at) = obj.get_mut("action_types").and_then(|v| v.as_object_mut()) {
                    let count = at
                        .get(&action.action_type)
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    at.insert(action.action_type.clone(), json!(count + 1));
                }

                obj.insert("last_action_time".into(), json!(&action.timestamp));
            }
        }

        let mut result: Vec<Value> = stats.into_values().collect();
        result.sort_by(|a, b| {
            b["total_actions"]
                .as_u64()
                .unwrap_or(0)
                .cmp(&a["total_actions"].as_u64().unwrap_or(0))
        });
        result
    }

    // ------------------------------------------------------------------
    // Log cleanup
    // ------------------------------------------------------------------

    /// Remove run logs (but not config / profiles) for a simulation.
    pub async fn cleanup_simulation_logs(sim_dir: &Path, simulation_id: &str) -> Value {
        let files_to_delete = [
            "run_state.json",
            "simulation.log",
            "stdout.log",
            "stderr.log",
            "twitter_simulation.db",
            "reddit_simulation.db",
            "env_status.json",
        ];

        let dirs_to_clean = ["twitter", "reddit"];
        let mut cleaned = Vec::new();
        let mut errors = Vec::new();

        for f in &files_to_delete {
            let p = sim_dir.join(f);
            if p.exists() {
                match fs::remove_file(&p).await {
                    Ok(_) => cleaned.push(f.to_string()),
                    Err(e) => errors.push(format!("Failed to delete {}: {}", f, e)),
                }
            }
        }

        for d in &dirs_to_clean {
            let actions_file = sim_dir.join(d).join("actions.jsonl");
            if actions_file.exists() {
                match fs::remove_file(&actions_file).await {
                    Ok(_) => cleaned.push(format!("{}/actions.jsonl", d)),
                    Err(e) => errors.push(format!("Failed to delete {}/actions.jsonl: {}", d, e)),
                }
            }
        }

        // Also remove round log files
        if let Ok(mut entries) = fs::read_dir(sim_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("round_") && name.ends_with(".json") {
                    if let Err(e) = fs::remove_file(entry.path()).await {
                        errors.push(format!("Failed to delete {}: {}", name, e));
                    } else {
                        cleaned.push(name);
                    }
                }
            }
        }

        // Clear in-memory state
        RUN_STATES.remove(simulation_id);

        info!(
            "Cleaned simulation logs: {}, deleted: {:?}",
            simulation_id, cleaned
        );

        json!({
            "success": errors.is_empty(),
            "cleaned_files": cleaned,
            "errors": if errors.is_empty() { Value::Null } else { json!(errors) },
        })
    }

    // ------------------------------------------------------------------
    // Cleanup all simulations (for server shutdown)
    // ------------------------------------------------------------------

    /// Register cleanup handlers. In Rust we use `tokio::signal` instead
    /// of `atexit` / `signal.signal`.
    pub fn register_cleanup() {
        // This is a no-op placeholder; the actual cleanup is triggered by
        // the axum shutdown signal handler calling `cleanup_all_simulations`.
        // The caller should hook into the server's shutdown lifecycle.
    }

    /// Stop all running simulations. Idempotent.
    pub async fn cleanup_all_simulations() {
        {
            let done = *CLEANUP_DONE.read().await;
            if done {
                return;
            }
        }
        {
            let mut done = CLEANUP_DONE.write().await;
            *done = true;
        }

        let sim_ids: Vec<String> = SIM_HANDLES.iter().map(|e| e.key().clone()).collect();
        if sim_ids.is_empty() {
            return;
        }

        info!("Cleaning up {} simulation(s)...", sim_ids.len());

        for sim_id in &sim_ids {
            // Set stop flag
            if let Some(flag) = STOP_FLAGS.get(sim_id) {
                let mut w = flag.write().await;
                *w = true;
            }
            if let Some(notify) = PAUSE_NOTIFIERS.get(sim_id) {
                notify.notify_one();
            }
        }

        // Wait for handles
        for sim_id in &sim_ids {
            if let Some((_, handle)) = SIM_HANDLES.remove(sim_id) {
                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
            }

            // Update state
            if let Some(mut entry) = RUN_STATES.get_mut(sim_id) {
                entry.runner_status = RunnerStatus::Stopped;
                entry.twitter_running = false;
                entry.reddit_running = false;
                entry.completed_at = Some(Utc::now().to_rfc3339());
                entry.error = Some("Server shutdown, simulation terminated".to_string());
            }
        }

        // Cleanup maps
        STOP_FLAGS.clear();
        PAUSE_FLAGS.clear();
        PAUSE_NOTIFIERS.clear();

        info!("Simulation cleanup complete");
    }

    // ------------------------------------------------------------------
    // The main simulation loop (runs in a spawned task)
    // ------------------------------------------------------------------

    /// Run simulation: the static entry point matching the existing API used
    /// by `api/simulation.rs`.
    pub async fn run_simulation(
        sim_dir: &Path,
        max_rounds: usize,
        enable_twitter: bool,
        enable_reddit: bool,
        progress_callback: Option<Box<dyn Fn(usize, &str) + Send>>,
    ) -> Result<Value> {
        // Load profiles
        let profiles = Self::load_profiles(sim_dir).await?;
        if profiles.is_empty() {
            bail!("No agent profiles found in simulation directory");
        }

        // Load config
        let config_path = sim_dir.join("simulation_config.json");
        let config: Value = if config_path.exists() {
            let data = fs::read_to_string(&config_path).await?;
            serde_json::from_str(&data)?
        } else {
            json!({})
        };

        // Determine platforms to run
        let platforms: Vec<&str> = {
            let mut p = Vec::new();
            if enable_twitter {
                p.push("twitter");
            }
            if enable_reddit {
                p.push("reddit");
            }
            if p.is_empty() {
                p.push("twitter");
            }
            p
        };

        // Get initial posts from config (if any)
        let initial_posts = config
            .get("initial_posts")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Get simulation topic
        let simulation_topic = config["simulation_topic"]
            .as_str()
            .unwrap_or("General social media simulation");

        // Initialise per-platform state
        let mut platform_states: HashMap<String, PlatformState> = HashMap::new();
        for &plat in &platforms {
            let mut ps = PlatformState::new();
            // Seed initial posts
            for post in &initial_posts {
                let content = post["content"].as_str().unwrap_or("Welcome to the simulation!");
                let author = post["author"].as_str().unwrap_or("system");
                ps.create_post(0, author, content, 0);
            }
            platform_states.insert(plat.to_string(), ps);
        }

        let mut all_actions: Vec<Value> = Vec::new();
        let mut total_action_count = 0usize;

        // Create platform action log directories and files
        for &plat in &platforms {
            let plat_dir = sim_dir.join(plat);
            fs::create_dir_all(&plat_dir).await?;
            // Write simulation_start event
            let start_event = json!({
                "event_type": "simulation_start",
                "platform": plat,
                "timestamp": Utc::now().to_rfc3339(),
                "total_rounds": max_rounds,
                "agent_count": profiles.len(),
            });
            Self::append_action_log(&plat_dir.join("actions.jsonl"), &start_event).await?;
        }

        // Build LLM client from env vars (or use a lightweight approach)
        let llm = Self::create_llm_from_env();

        // ------------------------------------------------------------------
        // Main round loop
        // ------------------------------------------------------------------
        for round in 1..=max_rounds {
            if let Some(ref cb) = progress_callback {
                cb(round, &format!("Round {}/{}", round, max_rounds));
            }

            let _round_start = Utc::now().to_rfc3339();
            let simulated_hour = round / 2; // ~30 min per round

            // Write round_start events
            for &plat in &platforms {
                let plat_dir = sim_dir.join(plat);
                let event = json!({
                    "event_type": "round_start",
                    "round": round,
                    "simulated_hours": simulated_hour,
                    "timestamp": Utc::now().to_rfc3339(),
                });
                Self::append_action_log(&plat_dir.join("actions.jsonl"), &event).await?;
            }

            let mut round_actions: Vec<Value> = Vec::new();

            // Each agent acts on each platform
            for profile in &profiles {
                for &plat in &platforms {
                    let ps = platform_states
                        .get_mut(plat)
                        .expect("platform state missing");

                    // Generate action for this agent
                    let action = Self::generate_agent_action(
                        &llm,
                        profile,
                        ps,
                        plat,
                        round,
                        simulation_topic,
                    )
                    .await;

                    match action {
                        Ok(agent_action) => {
                            // Apply action to platform state
                            Self::apply_action(ps, &agent_action, profile, round);

                            // Build action log entry
                            let log_entry = json!({
                                "round": round,
                                "timestamp": Utc::now().to_rfc3339(),
                                "platform": plat,
                                "agent_id": profile.agent_id,
                                "agent_name": &profile.name,
                                "action_type": &agent_action.action_type,
                                "action_args": &agent_action.action_args,
                                "result": &agent_action.result,
                                "success": agent_action.success,
                            });

                            // Write to platform-specific JSONL
                            let plat_dir = sim_dir.join(plat);
                            Self::append_action_log(
                                &plat_dir.join("actions.jsonl"),
                                &log_entry,
                            )
                            .await?;

                            round_actions.push(log_entry.clone());
                            all_actions.push(log_entry);
                            total_action_count += 1;
                        }
                        Err(e) => {
                            warn!(
                                "Failed to generate action for agent {} on {}: {}",
                                profile.name, plat, e
                            );
                            // Record as DO_NOTHING on failure
                            let fallback = json!({
                                "round": round,
                                "timestamp": Utc::now().to_rfc3339(),
                                "platform": plat,
                                "agent_id": profile.agent_id,
                                "agent_name": &profile.name,
                                "action_type": "DO_NOTHING",
                                "action_args": {"reason": format!("LLM error: {}", e)},
                                "success": false,
                            });
                            let plat_dir = sim_dir.join(plat);
                            Self::append_action_log(
                                &plat_dir.join("actions.jsonl"),
                                &fallback,
                            )
                            .await?;
                            round_actions.push(fallback);
                        }
                    }
                }
            }

            // Write round_end events
            for &plat in &platforms {
                let plat_dir = sim_dir.join(plat);
                let event = json!({
                    "event_type": "round_end",
                    "round": round,
                    "simulated_hours": simulated_hour,
                    "timestamp": Utc::now().to_rfc3339(),
                    "actions_count": round_actions.iter()
                        .filter(|a| a["platform"].as_str() == Some(plat))
                        .count(),
                });
                Self::append_action_log(&plat_dir.join("actions.jsonl"), &event).await?;
            }

            // Save per-round JSON log (used by SimulationManager.get_actions)
            let round_log_path = sim_dir.join(format!("round_{:03}.json", round));
            let log_json = serde_json::to_string_pretty(&round_actions)?;
            fs::write(&round_log_path, &log_json).await?;

            // Brief yield between rounds
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // Write simulation_end events
        for &plat in &platforms {
            let plat_dir = sim_dir.join(plat);
            let event = json!({
                "event_type": "simulation_end",
                "platform": plat,
                "timestamp": Utc::now().to_rfc3339(),
                "total_rounds": max_rounds,
                "total_actions": all_actions.iter()
                    .filter(|a| a["platform"].as_str() == Some(plat))
                    .count(),
            });
            Self::append_action_log(&plat_dir.join("actions.jsonl"), &event).await?;
        }

        info!(
            "Simulation complete: {} rounds, {} total actions, {} agents",
            max_rounds,
            total_action_count,
            profiles.len()
        );

        Ok(json!({
            "total_rounds": max_rounds,
            "total_actions": total_action_count,
            "agents_count": profiles.len(),
        }))
    }

    // ------------------------------------------------------------------
    // Internal: The full simulation loop (for start_simulation API)
    // ------------------------------------------------------------------

    async fn run_simulation_loop(
        sim_dir: &Path,
        simulation_id: &str,
        _platform: &str,
        enable_twitter: bool,
        enable_reddit: bool,
        total_rounds: usize,
        minutes_per_round: usize,
        config: &Value,
        llm: LLMClient,
        stop_flag: Arc<RwLock<bool>>,
        pause_flag: Arc<RwLock<bool>>,
        pause_notify: Arc<Notify>,
        graph_memory_manager: Option<Arc<Mutex<ZepGraphMemoryManager>>>,
    ) -> Result<()> {
        // Update to running
        if let Some(mut entry) = RUN_STATES.get_mut(simulation_id) {
            entry.runner_status = RunnerStatus::Running;
            let _ = Self::save_run_state(sim_dir, &entry).await;
        }

        // Load profiles
        let profiles = Self::load_profiles(sim_dir).await?;
        if profiles.is_empty() {
            bail!("No agent profiles found");
        }

        let platforms: Vec<&str> = {
            let mut p = Vec::new();
            if enable_twitter { p.push("twitter"); }
            if enable_reddit { p.push("reddit"); }
            if p.is_empty() { p.push("twitter"); }
            p
        };

        // Get simulation topic
        let simulation_topic = config["simulation_topic"]
            .as_str()
            .unwrap_or("General social media simulation");

        let initial_posts = config
            .get("initial_posts")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Init platform states
        let mut platform_states: HashMap<String, PlatformState> = HashMap::new();
        for &plat in &platforms {
            let mut ps = PlatformState::new();
            for post in &initial_posts {
                let content = post["content"].as_str().unwrap_or("Welcome!");
                let author = post["author"].as_str().unwrap_or("system");
                ps.create_post(0, author, content, 0);
            }
            platform_states.insert(plat.to_string(), ps);

            // Create platform dir and write start event
            let plat_dir = sim_dir.join(plat);
            fs::create_dir_all(&plat_dir).await?;
            let event = json!({
                "event_type": "simulation_start",
                "platform": plat,
                "timestamp": Utc::now().to_rfc3339(),
                "total_rounds": total_rounds,
                "agent_count": profiles.len(),
            });
            Self::append_action_log(&plat_dir.join("actions.jsonl"), &event).await?;
        }

        // Round loop
        for round in 1..=total_rounds {
            // Check stop
            {
                let stopped = *stop_flag.read().await;
                if stopped {
                    info!("Simulation {} stopped by user at round {}", simulation_id, round);
                    break;
                }
            }

            // Check pause
            loop {
                let paused = *pause_flag.read().await;
                if !paused {
                    break;
                }
                info!("Simulation {} paused at round {}", simulation_id, round);
                pause_notify.notified().await;
                // After notification, re-check stop
                let stopped = *stop_flag.read().await;
                if stopped {
                    break;
                }
            }
            {
                let stopped = *stop_flag.read().await;
                if stopped {
                    break;
                }
            }

            let simulated_hour = if minutes_per_round > 0 {
                round * minutes_per_round / 60
            } else {
                round / 2
            };

            // round_start events
            for &plat in &platforms {
                let event = json!({
                    "event_type": "round_start",
                    "round": round,
                    "simulated_hours": simulated_hour,
                    "timestamp": Utc::now().to_rfc3339(),
                });
                Self::append_action_log(&sim_dir.join(plat).join("actions.jsonl"), &event).await?;
            }

            let mut round_actions: Vec<Value> = Vec::new();

            for profile in &profiles {
                for &plat in &platforms {
                    // Check stop mid-round
                    {
                        let stopped = *stop_flag.read().await;
                        if stopped { break; }
                    }

                    let ps = platform_states.get_mut(plat).unwrap();

                    let action_result = Self::generate_agent_action(
                        &llm,
                        profile,
                        ps,
                        plat,
                        round,
                        simulation_topic,
                    )
                    .await;

                    let log_entry = match action_result {
                        Ok(agent_action) => {
                            Self::apply_action(ps, &agent_action, profile, round);
                            json!({
                                "round": round,
                                "timestamp": Utc::now().to_rfc3339(),
                                "platform": plat,
                                "agent_id": profile.agent_id,
                                "agent_name": &profile.name,
                                "action_type": &agent_action.action_type,
                                "action_args": &agent_action.action_args,
                                "result": &agent_action.result,
                                "success": agent_action.success,
                            })
                        }
                        Err(e) => {
                            warn!("Action generation failed for {}: {}", profile.name, e);
                            json!({
                                "round": round,
                                "timestamp": Utc::now().to_rfc3339(),
                                "platform": plat,
                                "agent_id": profile.agent_id,
                                "agent_name": &profile.name,
                                "action_type": "DO_NOTHING",
                                "action_args": {"reason": format!("Error: {}", e)},
                                "success": false,
                            })
                        }
                    };

                    // Write to JSONL
                    Self::append_action_log(
                        &sim_dir.join(plat).join("actions.jsonl"),
                        &log_entry,
                    ).await?;

                    // Feed to graph memory updater
                    if let Some(ref gmm) = graph_memory_manager {
                        let m = gmm.lock().await;
                        if m.has_updater(simulation_id).await {
                            let _ = m.add_activity(
                                simulation_id,
                                super::zep_graph_memory_updater::AgentActivity {
                                    platform: plat.to_string(),
                                    agent_id: profile.agent_id as i64,
                                    agent_name: profile.name.clone(),
                                    action_type: log_entry["action_type"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    action_args: log_entry["action_args"]
                                        .as_object()
                                        .map(|m| {
                                            m.iter()
                                                .map(|(k, v)| (k.clone(), v.clone()))
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                    round_num: round as i64,
                                    timestamp: log_entry["timestamp"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                },
                            ).await;
                        }
                    }

                    // Update in-memory run state
                    if let Some(mut entry) = RUN_STATES.get_mut(simulation_id) {
                        let action = AgentAction {
                            round_num: round,
                            timestamp: log_entry["timestamp"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            platform: plat.to_string(),
                            agent_id: profile.agent_id,
                            agent_name: profile.name.clone(),
                            action_type: log_entry["action_type"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            action_args: log_entry["action_args"]
                                .as_object()
                                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                                .unwrap_or_default(),
                            result: log_entry["result"].as_str().map(|s| s.to_string()),
                            success: log_entry["success"].as_bool().unwrap_or(true),
                        };
                        entry.add_action(action);
                    }

                    round_actions.push(log_entry);
                }
            }

            // round_end events
            for &plat in &platforms {
                let plat_action_count = round_actions
                    .iter()
                    .filter(|a| a["platform"].as_str() == Some(plat))
                    .count();
                let event = json!({
                    "event_type": "round_end",
                    "round": round,
                    "simulated_hours": simulated_hour,
                    "actions_count": plat_action_count,
                    "timestamp": Utc::now().to_rfc3339(),
                });
                Self::append_action_log(&sim_dir.join(plat).join("actions.jsonl"), &event).await?;
            }

            // Save per-round JSON log
            let round_log = sim_dir.join(format!("round_{:03}.json", round));
            let json_str = serde_json::to_string_pretty(&round_actions)?;
            fs::write(&round_log, &json_str).await?;

            // Update run state
            if let Some(mut entry) = RUN_STATES.get_mut(simulation_id) {
                entry.current_round = round;
                entry.simulated_hours = simulated_hour;
                if enable_twitter {
                    entry.twitter_current_round = round;
                    entry.twitter_simulated_hours = simulated_hour;
                }
                if enable_reddit {
                    entry.reddit_current_round = round;
                    entry.reddit_simulated_hours = simulated_hour;
                }
                let _ = Self::save_run_state(sim_dir, &entry).await;
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // simulation_end events
        for &plat in &platforms {
            let plat_dir = sim_dir.join(plat);
            let actions_file = plat_dir.join("actions.jsonl");
            // Count total actions for this platform
            let total_plat_actions = if let Ok(data) = fs::read_to_string(&actions_file).await {
                data.lines()
                    .filter(|l| {
                        serde_json::from_str::<Value>(l)
                            .ok()
                            .map(|v| v.get("event_type").is_none() && v.get("agent_id").is_some())
                            .unwrap_or(false)
                    })
                    .count()
            } else {
                0
            };

            let event = json!({
                "event_type": "simulation_end",
                "platform": plat,
                "timestamp": Utc::now().to_rfc3339(),
                "total_rounds": total_rounds,
                "total_actions": total_plat_actions,
            });
            Self::append_action_log(&actions_file, &event).await?;
        }

        // Final state update
        if let Some(mut entry) = RUN_STATES.get_mut(simulation_id) {
            let was_stopped = *stop_flag.read().await;
            if was_stopped {
                entry.runner_status = RunnerStatus::Stopped;
            } else {
                entry.runner_status = RunnerStatus::Completed;
            }
            entry.twitter_running = false;
            entry.reddit_running = false;
            entry.twitter_completed = enable_twitter && !was_stopped;
            entry.reddit_completed = enable_reddit && !was_stopped;
            entry.completed_at = Some(Utc::now().to_rfc3339());
            let _ = Self::save_run_state(sim_dir, &entry).await;
        }

        info!(
            "Simulation loop finished: {}, rounds={}",
            simulation_id, total_rounds
        );
        Ok(())
    }

    // ------------------------------------------------------------------
    // Agent action generation via LLM
    // ------------------------------------------------------------------

    async fn generate_agent_action(
        llm: &LLMClient,
        profile: &AgentProfile,
        platform_state: &PlatformState,
        platform: &str,
        round: usize,
        simulation_topic: &str,
    ) -> Result<AgentAction> {
        // Build the feed the agent sees
        let feed = platform_state.build_feed(profile.agent_id, 10);

        // Build persona description
        let persona_desc = if !profile.persona.is_empty() {
            profile.persona.clone()
        } else {
            let traits = if profile.personality_traits.is_empty() {
                "general".to_string()
            } else {
                profile.personality_traits.join(", ")
            };
            let interests = if profile.interests.is_empty() {
                "various topics".to_string()
            } else {
                profile.interests.join(", ")
            };
            format!(
                "Name: {}. Bio: {}. Traits: {}. Interests: {}.",
                profile.name, profile.bio, traits, interests
            )
        };

        // Available post IDs for interaction
        let available_post_ids: Vec<usize> = platform_state
            .posts
            .iter()
            .rev()
            .take(20)
            .map(|p| p.post_id)
            .collect();

        // Available agent names for follow/mute
        let _other_agent_names: Vec<String> = vec![]; // We don't have the full list here, but LLM can pick from feed

        let system_prompt = format!(
            r#"You are simulating a social media user on {platform}.

Your persona:
{persona_desc}

Simulation topic: {simulation_topic}
Current round: {round}

Your current timeline (most recent posts):
{feed}

Available post IDs for interaction: {available_ids}

Choose ONE action to perform. Respond with valid JSON only.

Available actions:
1. CREATE_POST - Create a new post
   {{"action_type": "CREATE_POST", "content": "your post text"}}
2. LIKE_POST - Like an existing post
   {{"action_type": "LIKE_POST", "post_id": <number>}}
3. DISLIKE_POST - Dislike an existing post
   {{"action_type": "DISLIKE_POST", "post_id": <number>}}
4. REPOST - Repost an existing post
   {{"action_type": "REPOST", "post_id": <number>}}
5. CREATE_COMMENT - Comment on a post
   {{"action_type": "CREATE_COMMENT", "post_id": <number>, "content": "your comment"}}
6. FOLLOW - Follow a user
   {{"action_type": "FOLLOW", "target_user_name": "username"}}
7. DO_NOTHING - Skip this round
   {{"action_type": "DO_NOTHING"}}

Rules:
- Stay in character as your persona.
- Make decisions that align with your personality and interests.
- Your content should be relevant to the simulation topic and/or your interests.
- If there are interesting posts on your timeline, you may interact with them.
- You can create new posts about topics you care about.
- Keep post content under 280 characters.
- Respond with ONLY the JSON object, no other text."#,
            platform = platform,
            persona_desc = persona_desc,
            simulation_topic = simulation_topic,
            round = round,
            feed = feed,
            available_ids = format!("{:?}", available_post_ids),
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: system_prompt,
        }];

        let response = llm.chat_json(&messages, 0.8, 512).await?;

        // Parse action
        let action_type = response["action_type"]
            .as_str()
            .unwrap_or("DO_NOTHING")
            .to_string();

        let mut action_args = HashMap::new();
        let mut result_text: Option<String>;

        match action_type.as_str() {
            "CREATE_POST" => {
                let content = response["content"]
                    .as_str()
                    .unwrap_or("(empty post)")
                    .to_string();
                action_args.insert("content".to_string(), json!(&content));
                result_text = Some(format!("Created post: {}", content));
            }
            "LIKE_POST" => {
                let pid = response["post_id"].as_u64().unwrap_or(0) as usize;
                action_args.insert("post_id".to_string(), json!(pid));
                if let Some(post) = platform_state.get_post(pid) {
                    action_args.insert("post_content".to_string(), json!(&post.content));
                    action_args.insert("post_author_name".to_string(), json!(&post.author_name));
                }
                result_text = Some(format!("Liked post #{}", pid));
            }
            "DISLIKE_POST" => {
                let pid = response["post_id"].as_u64().unwrap_or(0) as usize;
                action_args.insert("post_id".to_string(), json!(pid));
                if let Some(post) = platform_state.get_post(pid) {
                    action_args.insert("post_content".to_string(), json!(&post.content));
                    action_args.insert("post_author_name".to_string(), json!(&post.author_name));
                }
                result_text = Some(format!("Disliked post #{}", pid));
            }
            "REPOST" => {
                let pid = response["post_id"].as_u64().unwrap_or(0) as usize;
                action_args.insert("post_id".to_string(), json!(pid));
                if let Some(post) = platform_state.get_post(pid) {
                    action_args.insert("original_content".to_string(), json!(&post.content));
                    action_args
                        .insert("original_author_name".to_string(), json!(&post.author_name));
                }
                result_text = Some(format!("Reposted post #{}", pid));
            }
            "QUOTE_POST" => {
                let pid = response["post_id"].as_u64().unwrap_or(0) as usize;
                let content = response["content"]
                    .as_str()
                    .or_else(|| response["quote_content"].as_str())
                    .unwrap_or("")
                    .to_string();
                action_args.insert("post_id".to_string(), json!(pid));
                action_args.insert("content".to_string(), json!(&content));
                action_args.insert("quote_content".to_string(), json!(&content));
                if let Some(post) = platform_state.get_post(pid) {
                    action_args.insert("original_content".to_string(), json!(&post.content));
                    action_args
                        .insert("original_author_name".to_string(), json!(&post.author_name));
                }
                result_text = Some(format!("Quote-posted #{}: {}", pid, content));
            }
            "CREATE_COMMENT" => {
                let pid = response["post_id"].as_u64().unwrap_or(0) as usize;
                let content = response["content"]
                    .as_str()
                    .unwrap_or("(empty comment)")
                    .to_string();
                action_args.insert("post_id".to_string(), json!(pid));
                action_args.insert("content".to_string(), json!(&content));
                if let Some(post) = platform_state.get_post(pid) {
                    action_args.insert("post_content".to_string(), json!(&post.content));
                    action_args.insert("post_author_name".to_string(), json!(&post.author_name));
                }
                result_text = Some(format!("Commented on #{}: {}", pid, content));
            }
            "FOLLOW" => {
                let target = response["target_user_name"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                action_args.insert("target_user_name".to_string(), json!(&target));
                result_text = Some(format!("Followed @{}", target));
            }
            "MUTE" => {
                let target = response["target_user_name"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                action_args.insert("target_user_name".to_string(), json!(&target));
                result_text = Some(format!("Muted @{}", target));
            }
            "SEARCH_POSTS" => {
                let query = response["query"]
                    .as_str()
                    .or_else(|| response["keyword"].as_str())
                    .unwrap_or("")
                    .to_string();
                action_args.insert("query".to_string(), json!(&query));
                result_text = Some(format!("Searched for: {}", query));
            }
            "DO_NOTHING" => {
                let reason = response["reason"]
                    .as_str()
                    .unwrap_or("No action this round")
                    .to_string();
                action_args.insert("reason".to_string(), json!(&reason));
                result_text = Some("Did nothing".to_string());
            }
            _ => {
                action_args.insert("raw_response".to_string(), response.clone());
                result_text = Some(format!("Unknown action: {}", action_type));
            }
        }

        Ok(AgentAction {
            round_num: round,
            timestamp: Utc::now().to_rfc3339(),
            platform: platform.to_string(),
            agent_id: profile.agent_id,
            agent_name: profile.name.clone(),
            action_type,
            action_args,
            result: result_text,
            success: true,
        })
    }

    /// Apply a generated action to the platform state.
    fn apply_action(
        ps: &mut PlatformState,
        action: &AgentAction,
        profile: &AgentProfile,
        round: usize,
    ) {
        match action.action_type.as_str() {
            "CREATE_POST" => {
                let content = action
                    .action_args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(post)");
                ps.create_post(profile.agent_id, &profile.name, content, round);
            }
            "LIKE_POST" => {
                if let Some(pid) = action.action_args.get("post_id").and_then(|v| v.as_u64()) {
                    ps.like_post(pid as usize, profile.agent_id);
                }
            }
            "DISLIKE_POST" => {
                if let Some(pid) = action.action_args.get("post_id").and_then(|v| v.as_u64()) {
                    ps.dislike_post(pid as usize, profile.agent_id);
                }
            }
            "REPOST" => {
                if let Some(pid) = action.action_args.get("post_id").and_then(|v| v.as_u64()) {
                    ps.repost(pid as usize, profile.agent_id);
                }
            }
            "QUOTE_POST" => {
                // A quote post creates a new post referencing the original
                let content = action
                    .action_args
                    .get("content")
                    .or_else(|| action.action_args.get("quote_content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("(quote)");
                ps.create_post(profile.agent_id, &profile.name, content, round);
            }
            "CREATE_COMMENT" => {
                if let Some(pid) = action.action_args.get("post_id").and_then(|v| v.as_u64()) {
                    let content = action
                        .action_args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(comment)");
                    ps.add_comment(pid as usize, profile.agent_id, &profile.name, content, round);
                }
            }
            "FOLLOW" => {
                // We need target agent_id; for simplicity, find by name
                let target_name = action
                    .action_args
                    .get("target_user_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                // Find target_id from posts
                if let Some(target_post) = ps.posts.iter().find(|p| p.author_name == target_name) {
                    ps.follow(profile.agent_id, target_post.author_id);
                }
            }
            "MUTE" => {
                let target_name = action
                    .action_args
                    .get("target_user_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Some(target_post) = ps.posts.iter().find(|p| p.author_name == target_name) {
                    ps.mute(profile.agent_id, target_post.author_id);
                }
            }
            _ => {
                // DO_NOTHING, SEARCH_POSTS, etc. — no state change
            }
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Load agent profiles from the simulation directory.
    async fn load_profiles(sim_dir: &Path) -> Result<Vec<AgentProfile>> {
        // Try reddit_profiles.json first, then twitter_profiles.json
        let reddit_path = sim_dir.join("reddit_profiles.json");
        let twitter_path = sim_dir.join("twitter_profiles.json");
        let generic_path = sim_dir.join("profiles.json");

        let profiles_raw: Vec<Value> = if reddit_path.exists() {
            let data = fs::read_to_string(&reddit_path)
                .await
                .context("Failed to read reddit_profiles.json")?;
            serde_json::from_str(&data).context("Failed to parse reddit_profiles.json")?
        } else if twitter_path.exists() {
            let data = fs::read_to_string(&twitter_path)
                .await
                .context("Failed to read twitter_profiles.json")?;
            serde_json::from_str(&data).context("Failed to parse twitter_profiles.json")?
        } else if generic_path.exists() {
            let data = fs::read_to_string(&generic_path)
                .await
                .context("Failed to read profiles.json")?;
            serde_json::from_str(&data).context("Failed to parse profiles.json")?
        } else {
            bail!(
                "No profile files found in {:?}. Expected reddit_profiles.json, twitter_profiles.json, or profiles.json",
                sim_dir
            );
        };

        let mut profiles = Vec::new();
        for (idx, raw) in profiles_raw.iter().enumerate() {
            let agent_id = raw["user_id"]
                .as_u64()
                .or_else(|| raw["agent_id"].as_u64())
                .unwrap_or(idx as u64 + 1) as usize;

            let name = raw["name"]
                .as_str()
                .or_else(|| raw["username"].as_str())
                .unwrap_or(&format!("Agent_{}", agent_id))
                .to_string();

            let persona = raw["persona"]
                .as_str()
                .unwrap_or("")
                .to_string();

            let bio = raw["bio"]
                .as_str()
                .or_else(|| raw["description"].as_str())
                .unwrap_or("")
                .to_string();

            let interests: Vec<String> = raw["interests"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let personality_traits: Vec<String> = raw["personality_traits"]
                .as_array()
                .or_else(|| raw["traits"].as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let mut extra = HashMap::new();
            if let Some(obj) = raw.as_object() {
                for (k, v) in obj {
                    if !["user_id", "agent_id", "name", "username", "persona", "bio",
                         "description", "interests", "personality_traits", "traits"]
                        .contains(&k.as_str())
                    {
                        extra.insert(k.clone(), v.clone());
                    }
                }
            }

            profiles.push(AgentProfile {
                agent_id,
                name,
                persona,
                bio,
                interests,
                personality_traits,
                extra,
            });
        }

        Ok(profiles)
    }

    /// Create an LLM client from environment variables.
    fn create_llm_from_env() -> LLMClient {
        let api_key = std::env::var("LLM_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .unwrap_or_default();
        let base_url = std::env::var("LLM_BASE_URL")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let model = std::env::var("LLM_MODEL_NAME")
            .or_else(|_| std::env::var("OPENAI_MODEL"))
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());

        LLMClient::new(&api_key, &base_url, &model)
    }

    /// Append a single JSON line to an actions.jsonl file.
    async fn append_action_log(path: &Path, entry: &Value) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;

        let line = serde_json::to_string(entry)? + "\n";
        file.write_all(line.as_bytes()).await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Interview helpers (IPC-based — delegates to simulation_ipc)
    // ------------------------------------------------------------------

    /// Check if the simulation environment is alive (IPC-based).
    pub async fn check_env_alive(sim_dir: &Path) -> bool {
        let status_file = sim_dir.join("env_status.json");
        if !status_file.exists() {
            return false;
        }
        match fs::read_to_string(&status_file).await {
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(v) => v["status"].as_str() == Some("alive"),
                Err(_) => false,
            },
            Err(_) => false,
        }
    }

    /// Get detailed environment status.
    pub async fn get_env_status_detail(sim_dir: &Path) -> Value {
        let status_file = sim_dir.join("env_status.json");
        let default = json!({
            "status": "stopped",
            "twitter_available": false,
            "reddit_available": false,
            "timestamp": null,
        });

        if !status_file.exists() {
            return default;
        }

        match fs::read_to_string(&status_file).await {
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(status) => json!({
                    "status": status["status"].as_str().unwrap_or("stopped"),
                    "twitter_available": status["twitter_available"].as_bool().unwrap_or(false),
                    "reddit_available": status["reddit_available"].as_bool().unwrap_or(false),
                    "timestamp": status["timestamp"],
                }),
                Err(_) => default,
            },
            Err(_) => default,
        }
    }

    /// Interview a single agent via IPC.
    pub async fn interview_agent(
        sim_dir: &Path,
        agent_id: i64,
        prompt: &str,
        platform: Option<&str>,
        timeout_secs: f64,
    ) -> Result<Value> {
        use super::simulation_ipc::SimulationIPCClient;

        let ipc = SimulationIPCClient::new(sim_dir).await?;
        if !ipc.check_env_alive().await {
            bail!("Simulation environment is not running or has been shut down");
        }

        info!(
            "Sending interview command: agent_id={}, platform={:?}",
            agent_id, platform
        );

        let response = ipc
            .send_interview(agent_id, prompt, platform, timeout_secs)
            .await?;

        if response.status == super::simulation_ipc::CommandStatus::Completed {
            Ok(json!({
                "success": true,
                "agent_id": agent_id,
                "prompt": prompt,
                "result": response.result,
                "timestamp": response.timestamp,
            }))
        } else {
            Ok(json!({
                "success": false,
                "agent_id": agent_id,
                "prompt": prompt,
                "error": response.error,
                "timestamp": response.timestamp,
            }))
        }
    }

    /// Batch interview multiple agents.
    pub async fn interview_agents_batch(
        sim_dir: &Path,
        interviews: Vec<Value>,
        platform: Option<&str>,
        timeout_secs: f64,
    ) -> Result<Value> {
        use super::simulation_ipc::SimulationIPCClient;

        let ipc = SimulationIPCClient::new(sim_dir).await?;
        if !ipc.check_env_alive().await {
            bail!("Simulation environment is not running");
        }

        let count = interviews.len();
        let response = ipc
            .send_batch_interview(interviews, platform, timeout_secs)
            .await?;

        if response.status == super::simulation_ipc::CommandStatus::Completed {
            Ok(json!({
                "success": true,
                "interviews_count": count,
                "result": response.result,
                "timestamp": response.timestamp,
            }))
        } else {
            Ok(json!({
                "success": false,
                "interviews_count": count,
                "error": response.error,
                "timestamp": response.timestamp,
            }))
        }
    }

    /// Interview all agents with the same prompt.
    pub async fn interview_all_agents(
        sim_dir: &Path,
        prompt: &str,
        platform: Option<&str>,
        timeout_secs: f64,
    ) -> Result<Value> {
        // Read agent configs from simulation_config.json
        let config_path = sim_dir.join("simulation_config.json");
        if !config_path.exists() {
            bail!("Simulation config not found");
        }

        let config_data = fs::read_to_string(&config_path).await?;
        let config: Value = serde_json::from_str(&config_data)?;

        let agent_configs = config["agent_configs"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        if agent_configs.is_empty() {
            bail!("No agents found in simulation config");
        }

        let interviews: Vec<Value> = agent_configs
            .iter()
            .filter_map(|ac| {
                ac["agent_id"]
                    .as_u64()
                    .map(|id| json!({"agent_id": id, "prompt": prompt}))
            })
            .collect();

        Self::interview_agents_batch(sim_dir, interviews, platform, timeout_secs).await
    }

    /// Close the simulation environment via IPC.
    pub async fn close_simulation_env(sim_dir: &Path, timeout_secs: f64) -> Result<Value> {
        use super::simulation_ipc::SimulationIPCClient;

        let ipc = SimulationIPCClient::new(sim_dir).await?;
        if !ipc.check_env_alive().await {
            return Ok(json!({
                "success": true,
                "message": "Environment is already closed"
            }));
        }

        info!("Sending close-env command");

        match ipc.send_close_env(timeout_secs).await {
            Ok(response) => Ok(json!({
                "success": response.status == super::simulation_ipc::CommandStatus::Completed,
                "message": "Close-env command sent",
                "result": response.result,
                "timestamp": response.timestamp,
            })),
            Err(_) => Ok(json!({
                "success": true,
                "message": "Close-env command sent (response timed out, environment may be closing)"
            })),
        }
    }

    /// Get interview history from SQLite databases.
    pub async fn get_interview_history(
        sim_dir: &Path,
        platform_filter: Option<&str>,
        agent_id: Option<i64>,
        limit: usize,
    ) -> Vec<Value> {
        let platforms: Vec<&str> = match platform_filter {
            Some(p) => vec![p],
            None => vec!["twitter", "reddit"],
        };

        let mut results = Vec::new();

        for plat in &platforms {
            let db_path = sim_dir.join(format!("{}_simulation.db", plat));
            if !db_path.exists() {
                continue;
            }

            // SQLite access via a blocking task (we don't have rusqlite in deps)
            // Instead, read from interview_history.json if it exists
            let history_file = sim_dir.join("interview_history.json");
            if let Ok(data) = fs::read_to_string(&history_file).await {
                if let Ok(history) = serde_json::from_str::<Vec<Value>>(&data) {
                    for entry in history {
                        let entry_platform = entry["platform"].as_str().unwrap_or("");
                        if !platform_filter.map(|pf| pf == entry_platform).unwrap_or(true) {
                            continue;
                        }
                        if let Some(aid) = agent_id {
                            if entry["agent_id"].as_i64() != Some(aid) {
                                continue;
                            }
                        }
                        results.push(entry);
                    }
                }
            }
        }

        // Sort by timestamp descending
        results.sort_by(|a, b| {
            let ta = a["timestamp"].as_str().unwrap_or("");
            let tb = b["timestamp"].as_str().unwrap_or("");
            tb.cmp(ta)
        });

        if results.len() > limit {
            results.truncate(limit);
        }

        results
    }
}
