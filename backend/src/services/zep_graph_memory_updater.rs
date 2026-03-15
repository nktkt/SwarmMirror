//! Zep Graph Memory Updater service.
//!
//! Monitors agent activities during a simulation and updates the Zep knowledge
//! graph in real-time. Activities are buffered per-platform and sent in batches
//! to avoid excessive API calls.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use super::zep_client::ZepClient;

// ---------------------------------------------------------------------------
// AgentActivity
// ---------------------------------------------------------------------------

/// A record of an agent's action within the simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActivity {
    /// Platform name (`twitter` / `reddit`)
    pub platform: String,
    /// Agent ID
    pub agent_id: i64,
    /// Agent display name
    pub agent_name: String,
    /// Action type (e.g. `CREATE_POST`, `LIKE_POST`, etc.)
    pub action_type: String,
    /// Action arguments containing contextual data
    pub action_args: HashMap<String, Value>,
    /// Simulation round number
    pub round_num: i64,
    /// ISO-8601 timestamp
    pub timestamp: String,
}

impl AgentActivity {
    /// Convert the activity to a natural-language episode text suitable for
    /// ingestion by Zep's graph API.
    pub fn to_episode_text(&self) -> String {
        let description = match self.action_type.as_str() {
            "CREATE_POST" => self.describe_create_post(),
            "LIKE_POST" => self.describe_like_post(),
            "DISLIKE_POST" => self.describe_dislike_post(),
            "REPOST" => self.describe_repost(),
            "QUOTE_POST" => self.describe_quote_post(),
            "FOLLOW" => self.describe_follow(),
            "CREATE_COMMENT" => self.describe_create_comment(),
            "LIKE_COMMENT" => self.describe_like_comment(),
            "DISLIKE_COMMENT" => self.describe_dislike_comment(),
            "SEARCH_POSTS" => self.describe_search(),
            "SEARCH_USER" => self.describe_search_user(),
            "MUTE" => self.describe_mute(),
            _ => self.describe_generic(),
        };

        format!("{}: {}", self.agent_name, description)
    }

    // -- private description helpers ----------------------------------------

    fn get_arg_str(&self, key: &str) -> String {
        self.action_args
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn describe_create_post(&self) -> String {
        let content = self.get_arg_str("content");
        if !content.is_empty() {
            format!("\u{53d1}\u{5e03}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}\u{ff1a}\u{300c}{content}\u{300d}")
        } else {
            "\u{53d1}\u{5e03}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}".to_string()
        }
    }

    fn describe_like_post(&self) -> String {
        let post_content = self.get_arg_str("post_content");
        let post_author = self.get_arg_str("post_author_name");

        match (!post_content.is_empty(), !post_author.is_empty()) {
            (true, true) => format!("\u{70b9}\u{8d5e}\u{4e86}{post_author}\u{7684}\u{5e16}\u{5b50}\u{ff1a}\u{300c}{post_content}\u{300d}"),
            (true, false) => format!("\u{70b9}\u{8d5e}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}\u{ff1a}\u{300c}{post_content}\u{300d}"),
            (false, true) => format!("\u{70b9}\u{8d5e}\u{4e86}{post_author}\u{7684}\u{4e00}\u{6761}\u{5e16}\u{5b50}"),
            (false, false) => "\u{70b9}\u{8d5e}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}".to_string(),
        }
    }

    fn describe_dislike_post(&self) -> String {
        let post_content = self.get_arg_str("post_content");
        let post_author = self.get_arg_str("post_author_name");

        match (!post_content.is_empty(), !post_author.is_empty()) {
            (true, true) => format!("\u{8e29}\u{4e86}{post_author}\u{7684}\u{5e16}\u{5b50}\u{ff1a}\u{300c}{post_content}\u{300d}"),
            (true, false) => format!("\u{8e29}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}\u{ff1a}\u{300c}{post_content}\u{300d}"),
            (false, true) => format!("\u{8e29}\u{4e86}{post_author}\u{7684}\u{4e00}\u{6761}\u{5e16}\u{5b50}"),
            (false, false) => "\u{8e29}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}".to_string(),
        }
    }

    fn describe_repost(&self) -> String {
        let original_content = self.get_arg_str("original_content");
        let original_author = self.get_arg_str("original_author_name");

        match (!original_content.is_empty(), !original_author.is_empty()) {
            (true, true) => format!("\u{8f6c}\u{53d1}\u{4e86}{original_author}\u{7684}\u{5e16}\u{5b50}\u{ff1a}\u{300c}{original_content}\u{300d}"),
            (true, false) => format!("\u{8f6c}\u{53d1}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}\u{ff1a}\u{300c}{original_content}\u{300d}"),
            (false, true) => format!("\u{8f6c}\u{53d1}\u{4e86}{original_author}\u{7684}\u{4e00}\u{6761}\u{5e16}\u{5b50}"),
            (false, false) => "\u{8f6c}\u{53d1}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}".to_string(),
        }
    }

    fn describe_quote_post(&self) -> String {
        let original_content = self.get_arg_str("original_content");
        let original_author = self.get_arg_str("original_author_name");
        let quote_content = {
            let qc = self.get_arg_str("quote_content");
            if qc.is_empty() {
                self.get_arg_str("content")
            } else {
                qc
            }
        };

        let base = match (!original_content.is_empty(), !original_author.is_empty()) {
            (true, true) => format!("\u{5f15}\u{7528}\u{4e86}{original_author}\u{7684}\u{5e16}\u{5b50}\u{300c}{original_content}\u{300d}"),
            (true, false) => format!("\u{5f15}\u{7528}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}\u{300c}{original_content}\u{300d}"),
            (false, true) => format!("\u{5f15}\u{7528}\u{4e86}{original_author}\u{7684}\u{4e00}\u{6761}\u{5e16}\u{5b50}"),
            (false, false) => "\u{5f15}\u{7528}\u{4e86}\u{4e00}\u{6761}\u{5e16}\u{5b50}".to_string(),
        };

        if !quote_content.is_empty() {
            format!("{base}\u{ff0c}\u{5e76}\u{8bc4}\u{8bba}\u{9053}\u{ff1a}\u{300c}{quote_content}\u{300d}")
        } else {
            base
        }
    }

    fn describe_follow(&self) -> String {
        let target = self.get_arg_str("target_user_name");
        if !target.is_empty() {
            format!("\u{5173}\u{6ce8}\u{4e86}\u{7528}\u{6237}\u{300c}{target}\u{300d}")
        } else {
            "\u{5173}\u{6ce8}\u{4e86}\u{4e00}\u{4e2a}\u{7528}\u{6237}".to_string()
        }
    }

    fn describe_create_comment(&self) -> String {
        let content = self.get_arg_str("content");
        let post_content = self.get_arg_str("post_content");
        let post_author = self.get_arg_str("post_author_name");

        if !content.is_empty() {
            match (!post_content.is_empty(), !post_author.is_empty()) {
                (true, true) => format!("\u{5728}{post_author}\u{7684}\u{5e16}\u{5b50}\u{300c}{post_content}\u{300d}\u{4e0b}\u{8bc4}\u{8bba}\u{9053}\u{ff1a}\u{300c}{content}\u{300d}"),
                (true, false) => format!("\u{5728}\u{5e16}\u{5b50}\u{300c}{post_content}\u{300d}\u{4e0b}\u{8bc4}\u{8bba}\u{9053}\u{ff1a}\u{300c}{content}\u{300d}"),
                (false, true) => format!("\u{5728}{post_author}\u{7684}\u{5e16}\u{5b50}\u{4e0b}\u{8bc4}\u{8bba}\u{9053}\u{ff1a}\u{300c}{content}\u{300d}"),
                (false, false) => format!("\u{8bc4}\u{8bba}\u{9053}\u{ff1a}\u{300c}{content}\u{300d}"),
            }
        } else {
            "\u{53d1}\u{8868}\u{4e86}\u{8bc4}\u{8bba}".to_string()
        }
    }

    fn describe_like_comment(&self) -> String {
        let comment_content = self.get_arg_str("comment_content");
        let comment_author = self.get_arg_str("comment_author_name");

        match (!comment_content.is_empty(), !comment_author.is_empty()) {
            (true, true) => format!("\u{70b9}\u{8d5e}\u{4e86}{comment_author}\u{7684}\u{8bc4}\u{8bba}\u{ff1a}\u{300c}{comment_content}\u{300d}"),
            (true, false) => format!("\u{70b9}\u{8d5e}\u{4e86}\u{4e00}\u{6761}\u{8bc4}\u{8bba}\u{ff1a}\u{300c}{comment_content}\u{300d}"),
            (false, true) => format!("\u{70b9}\u{8d5e}\u{4e86}{comment_author}\u{7684}\u{4e00}\u{6761}\u{8bc4}\u{8bba}"),
            (false, false) => "\u{70b9}\u{8d5e}\u{4e86}\u{4e00}\u{6761}\u{8bc4}\u{8bba}".to_string(),
        }
    }

    fn describe_dislike_comment(&self) -> String {
        let comment_content = self.get_arg_str("comment_content");
        let comment_author = self.get_arg_str("comment_author_name");

        match (!comment_content.is_empty(), !comment_author.is_empty()) {
            (true, true) => format!("\u{8e29}\u{4e86}{comment_author}\u{7684}\u{8bc4}\u{8bba}\u{ff1a}\u{300c}{comment_content}\u{300d}"),
            (true, false) => format!("\u{8e29}\u{4e86}\u{4e00}\u{6761}\u{8bc4}\u{8bba}\u{ff1a}\u{300c}{comment_content}\u{300d}"),
            (false, true) => format!("\u{8e29}\u{4e86}{comment_author}\u{7684}\u{4e00}\u{6761}\u{8bc4}\u{8bba}"),
            (false, false) => "\u{8e29}\u{4e86}\u{4e00}\u{6761}\u{8bc4}\u{8bba}".to_string(),
        }
    }

    fn describe_search(&self) -> String {
        let query = {
            let q = self.get_arg_str("query");
            if q.is_empty() {
                self.get_arg_str("keyword")
            } else {
                q
            }
        };
        if !query.is_empty() {
            format!("\u{641c}\u{7d22}\u{4e86}\u{300c}{query}\u{300d}")
        } else {
            "\u{8fdb}\u{884c}\u{4e86}\u{641c}\u{7d22}".to_string()
        }
    }

    fn describe_search_user(&self) -> String {
        let query = {
            let q = self.get_arg_str("query");
            if q.is_empty() {
                self.get_arg_str("username")
            } else {
                q
            }
        };
        if !query.is_empty() {
            format!("\u{641c}\u{7d22}\u{4e86}\u{7528}\u{6237}\u{300c}{query}\u{300d}")
        } else {
            "\u{641c}\u{7d22}\u{4e86}\u{7528}\u{6237}".to_string()
        }
    }

    fn describe_mute(&self) -> String {
        let target = self.get_arg_str("target_user_name");
        if !target.is_empty() {
            format!("\u{5c4f}\u{853d}\u{4e86}\u{7528}\u{6237}\u{300c}{target}\u{300d}")
        } else {
            "\u{5c4f}\u{853d}\u{4e86}\u{4e00}\u{4e2a}\u{7528}\u{6237}".to_string()
        }
    }

    fn describe_generic(&self) -> String {
        format!("\u{6267}\u{884c}\u{4e86}{}\u{64cd}\u{4f5c}", self.action_type)
    }
}

// ---------------------------------------------------------------------------
// ZepGraphMemoryUpdater
// ---------------------------------------------------------------------------

/// Platform display-name mapping.
fn platform_display_name(platform: &str) -> &str {
    match platform.to_lowercase().as_str() {
        "twitter" => "\u{4e16}\u{754c}1",
        "reddit" => "\u{4e16}\u{754c}2",
        _ => platform.to_lowercase().leak(),
    }
}

/// Statistics for a single updater instance.
#[derive(Debug, Clone, Serialize)]
pub struct UpdaterStats {
    pub graph_id: String,
    pub batch_size: usize,
    pub total_activities: u64,
    pub batches_sent: u64,
    pub items_sent: u64,
    pub failed_count: u64,
    pub skipped_count: u64,
    pub queue_size: usize,
    pub buffer_sizes: HashMap<String, usize>,
    pub running: bool,
}

/// Monitors agent activities and batches them into Zep graph episodes.
///
/// Activities are grouped by platform and flushed when the per-platform
/// buffer reaches `BATCH_SIZE`.
pub struct ZepGraphMemoryUpdater {
    graph_id: String,
    client: ZepClient,

    /// Channel sender for queueing activities.
    tx: mpsc::UnboundedSender<AgentActivity>,

    /// Per-platform activity buffers (shared with worker task).
    platform_buffers: Arc<Mutex<HashMap<String, Vec<AgentActivity>>>>,

    /// Control flag shared with worker.
    running: Arc<RwLock<bool>>,

    /// Handle to the background worker task.
    worker_handle: Option<JoinHandle<()>>,

    /// Statistics (shared with worker task).
    stats: Arc<Mutex<UpdaterStatsInner>>,
}

/// Internal mutable statistics.
#[derive(Debug, Default)]
struct UpdaterStatsInner {
    total_activities: u64,
    total_sent: u64,
    total_items_sent: u64,
    failed_count: u64,
    skipped_count: u64,
}

/// Batch size before flushing to Zep.
const BATCH_SIZE: usize = 5;
/// Minimum interval between Zep API calls (seconds).
const SEND_INTERVAL_SECS: f64 = 0.5;
/// Max retries for a failed batch send.
const MAX_RETRIES: u32 = 3;
/// Base retry delay (seconds), multiplied by attempt number.
const RETRY_DELAY_SECS: f64 = 2.0;

impl ZepGraphMemoryUpdater {
    /// Create a new updater. Call [`start`] to begin background processing.
    pub fn new(graph_id: &str, api_key: &str) -> Result<Self> {
        if api_key.is_empty() {
            anyhow::bail!("ZEP_API_KEY is not configured");
        }

        let client = ZepClient::new(api_key);
        let (tx, _rx) = mpsc::unbounded_channel::<AgentActivity>();

        let mut buffers = HashMap::new();
        buffers.insert("twitter".to_string(), Vec::new());
        buffers.insert("reddit".to_string(), Vec::new());
        let platform_buffers = Arc::new(Mutex::new(buffers));
        let running = Arc::new(RwLock::new(false));
        let stats = Arc::new(Mutex::new(UpdaterStatsInner::default()));

        info!(
            "ZepGraphMemoryUpdater initialized: graph_id={}, batch_size={}",
            graph_id, BATCH_SIZE
        );

        Ok(Self {
            graph_id: graph_id.to_string(),
            client,
            tx,
            platform_buffers,
            running,
            worker_handle: None,
            stats,
        })
    }

    /// Start the background worker task.
    pub fn start(&mut self) {
        let already_running = {
            let guard = self.running.try_read();
            guard.map(|g| *g).unwrap_or(false)
        };
        if already_running {
            return;
        }

        // We need a new channel because the old rx was moved into the
        // previous worker (if any). Re-create here.
        let (tx, rx) = mpsc::unbounded_channel::<AgentActivity>();
        self.tx = tx;

        {
            // Set running flag synchronously (blocking is fine at startup).
            let running = self.running.clone();
            tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let mut w = running.write().await;
                    *w = true;
                });
            });
        }

        let graph_id = self.graph_id.clone();
        let client = self.client.clone();
        let buffers = self.platform_buffers.clone();
        let running = self.running.clone();
        let stats = self.stats.clone();

        let handle = tokio::spawn(async move {
            worker_loop(rx, client, graph_id, buffers, running, stats).await;
        });

        self.worker_handle = Some(handle);
        info!("ZepGraphMemoryUpdater started: graph_id={}", self.graph_id);
    }

    /// Stop the background worker and flush remaining buffered activities.
    pub async fn stop(&mut self) {
        {
            let mut w = self.running.write().await;
            *w = false;
        }

        // Flush remaining activities
        self.flush_remaining().await;

        if let Some(handle) = self.worker_handle.take() {
            // Give it a generous timeout
            let _ = tokio::time::timeout(Duration::from_secs(10), handle).await;
        }

        let s = self.stats.lock().await;
        info!(
            "ZepGraphMemoryUpdater stopped: graph_id={}, \
             total_activities={}, batches_sent={}, items_sent={}, \
             failed={}, skipped={}",
            self.graph_id, s.total_activities, s.total_sent, s.total_items_sent,
            s.failed_count, s.skipped_count
        );
    }

    /// Enqueue an agent activity for processing.
    ///
    /// Activities of type `DO_NOTHING` are silently skipped.
    pub async fn add_activity(&self, activity: AgentActivity) {
        if activity.action_type == "DO_NOTHING" {
            let mut s = self.stats.lock().await;
            s.skipped_count += 1;
            return;
        }

        debug!(
            "Adding activity to Zep queue: {} - {}",
            activity.agent_name, activity.action_type
        );

        {
            let mut s = self.stats.lock().await;
            s.total_activities += 1;
        }

        if self.tx.send(activity).is_err() {
            warn!("Failed to enqueue activity (channel closed)");
        }
    }

    /// Build an [`AgentActivity`] from a raw dictionary (as parsed from
    /// `actions.jsonl`) and enqueue it.
    pub async fn add_activity_from_dict(&self, data: &Value, platform: &str) {
        // Skip event-type entries
        if data.get("event_type").is_some() {
            return;
        }

        let activity = AgentActivity {
            platform: platform.to_string(),
            agent_id: data
                .get("agent_id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            agent_name: data
                .get("agent_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            action_type: data
                .get("action_type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            action_args: data
                .get("action_args")
                .and_then(|v| serde_json::from_value::<HashMap<String, Value>>(v.clone()).ok())
                .unwrap_or_default(),
            round_num: data.get("round").and_then(|v| v.as_i64()).unwrap_or(0),
            timestamp: data
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or(&Utc::now().to_rfc3339())
                .to_string(),
        };

        self.add_activity(activity).await;
    }

    /// Flush all remaining queued and buffered activities to Zep
    /// (regardless of batch size).
    async fn flush_remaining(&self) {
        // Drain whatever is left on the channel into the buffers
        // (the channel may already be closed, so we just try).
        // Note: we cannot drain the rx here since it was moved into the
        // worker. The worker loop will drain on exit. We just flush the
        // buffers.

        let mut bufs = self.platform_buffers.lock().await;
        for (platform, buffer) in bufs.iter_mut() {
            if !buffer.is_empty() {
                let display_name = platform_display_name(platform);
                info!(
                    "Flushing {} remaining {} activities",
                    buffer.len(),
                    display_name
                );
                send_batch_activities(
                    &self.client,
                    &self.graph_id,
                    buffer,
                    platform,
                    &self.stats,
                )
                .await;
                buffer.clear();
            }
        }
    }

    /// Return a snapshot of the current statistics.
    pub async fn get_stats(&self) -> UpdaterStats {
        let s = self.stats.lock().await;
        let bufs = self.platform_buffers.lock().await;
        let buffer_sizes: HashMap<String, usize> =
            bufs.iter().map(|(k, v)| (k.clone(), v.len())).collect();
        let running = *self.running.read().await;

        UpdaterStats {
            graph_id: self.graph_id.clone(),
            batch_size: BATCH_SIZE,
            total_activities: s.total_activities,
            batches_sent: s.total_sent,
            items_sent: s.total_items_sent,
            failed_count: s.failed_count,
            skipped_count: s.skipped_count,
            queue_size: 0, // cannot observe unbounded channel length easily
            buffer_sizes,
            running,
        }
    }
}

// ---------------------------------------------------------------------------
// Background worker
// ---------------------------------------------------------------------------

/// The background event loop that drains the channel, buffers per platform,
/// and flushes to Zep when BATCH_SIZE is reached.
async fn worker_loop(
    mut rx: mpsc::UnboundedReceiver<AgentActivity>,
    client: ZepClient,
    graph_id: String,
    platform_buffers: Arc<Mutex<HashMap<String, Vec<AgentActivity>>>>,
    running: Arc<RwLock<bool>>,
    stats: Arc<Mutex<UpdaterStatsInner>>,
) {
    loop {
        let is_running = *running.read().await;

        tokio::select! {
            maybe_activity = rx.recv() => {
                match maybe_activity {
                    Some(activity) => {
                        let platform = activity.platform.to_lowercase();
                        let mut bufs = platform_buffers.lock().await;
                        let buf = bufs.entry(platform.clone()).or_insert_with(Vec::new);
                        buf.push(activity);

                        if buf.len() >= BATCH_SIZE {
                            let batch: Vec<AgentActivity> = buf.drain(..BATCH_SIZE).collect();
                            drop(bufs); // release lock before network I/O
                            send_batch_activities(&client, &graph_id, &batch, &platform, &stats).await;
                            sleep(Duration::from_secs_f64(SEND_INTERVAL_SECS)).await;
                        }
                    }
                    None => {
                        // Channel closed - drain buffers and exit
                        let mut bufs = platform_buffers.lock().await;
                        for (platform, buffer) in bufs.iter_mut() {
                            if !buffer.is_empty() {
                                let batch: Vec<AgentActivity> = buffer.drain(..).collect();
                                send_batch_activities(&client, &graph_id, &batch, platform, &stats).await;
                            }
                        }
                        break;
                    }
                }
            }
            _ = sleep(Duration::from_secs(1)) => {
                // Periodic wake-up to check the running flag
                if !is_running && rx.is_empty() {
                    // Drain any remaining buffered items
                    let mut bufs = platform_buffers.lock().await;
                    for (platform, buffer) in bufs.iter_mut() {
                        if !buffer.is_empty() {
                            let batch: Vec<AgentActivity> = buffer.drain(..).collect();
                            send_batch_activities(&client, &graph_id, &batch, platform, &stats).await;
                        }
                    }
                    break;
                }
            }
        }
    }
}

/// Send a batch of activities to the Zep graph as a single episode.
async fn send_batch_activities(
    client: &ZepClient,
    graph_id: &str,
    activities: &[AgentActivity],
    platform: &str,
    stats: &Arc<Mutex<UpdaterStatsInner>>,
) {
    if activities.is_empty() {
        return;
    }

    let episode_texts: Vec<String> = activities.iter().map(|a| a.to_episode_text()).collect();
    let combined_text = episode_texts.join("\n");

    for attempt in 0..MAX_RETRIES {
        match client
            .add_text_batch(graph_id, &[combined_text.clone()])
            .await
        {
            Ok(_) => {
                let mut s = stats.lock().await;
                s.total_sent += 1;
                s.total_items_sent += activities.len() as u64;

                let display_name = platform_display_name(platform);
                info!(
                    "Successfully sent {} {} activities to graph {}",
                    activities.len(),
                    display_name,
                    graph_id
                );
                debug!("Batch content preview: {}...", &combined_text[..combined_text.len().min(200)]);
                return;
            }
            Err(e) => {
                if attempt < MAX_RETRIES - 1 {
                    warn!(
                        "Failed to send batch to Zep (attempt {}/{}): {}",
                        attempt + 1,
                        MAX_RETRIES,
                        e
                    );
                    sleep(Duration::from_secs_f64(
                        RETRY_DELAY_SECS * (attempt as f64 + 1.0),
                    ))
                    .await;
                } else {
                    error!(
                        "Failed to send batch to Zep after {} retries: {}",
                        MAX_RETRIES, e
                    );
                    let mut s = stats.lock().await;
                    s.failed_count += 1;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ZepGraphMemoryManager
// ---------------------------------------------------------------------------

/// Manages multiple [`ZepGraphMemoryUpdater`] instances, one per simulation.
pub struct ZepGraphMemoryManager {
    updaters: Arc<Mutex<HashMap<String, ZepGraphMemoryUpdater>>>,
    stop_all_done: Arc<RwLock<bool>>,
}

impl ZepGraphMemoryManager {
    /// Create a new manager.
    pub fn new() -> Self {
        Self {
            updaters: Arc::new(Mutex::new(HashMap::new())),
            stop_all_done: Arc::new(RwLock::new(false)),
        }
    }

    /// Create a new updater for a simulation and start it.
    pub async fn create_updater(
        &self,
        simulation_id: &str,
        graph_id: &str,
        api_key: &str,
    ) -> Result<()> {
        let mut map = self.updaters.lock().await;

        // Stop existing updater for this simulation if present
        if let Some(existing) = map.get_mut(simulation_id) {
            existing.stop().await;
        }

        let mut updater = ZepGraphMemoryUpdater::new(graph_id, api_key)?;
        updater.start();
        map.insert(simulation_id.to_string(), updater);

        info!(
            "Created graph memory updater: simulation_id={}, graph_id={}",
            simulation_id, graph_id
        );
        Ok(())
    }

    /// Get a reference-counted handle to a simulation's updater.
    ///
    /// Because the updater is behind a `Mutex`, callers must lock
    /// `updaters` to interact with it. This method is a convenience
    /// that checks existence.
    pub async fn has_updater(&self, simulation_id: &str) -> bool {
        let map = self.updaters.lock().await;
        map.contains_key(simulation_id)
    }

    /// Add an activity to the updater for the given simulation.
    pub async fn add_activity(
        &self,
        simulation_id: &str,
        activity: AgentActivity,
    ) -> Result<()> {
        let map = self.updaters.lock().await;
        match map.get(simulation_id) {
            Some(updater) => {
                updater.add_activity(activity).await;
                Ok(())
            }
            None => {
                anyhow::bail!(
                    "No updater found for simulation_id={}",
                    simulation_id
                );
            }
        }
    }

    /// Stop and remove the updater for a specific simulation.
    pub async fn stop_updater(&self, simulation_id: &str) {
        let mut map = self.updaters.lock().await;
        if let Some(mut updater) = map.remove(simulation_id) {
            updater.stop().await;
            info!(
                "Stopped graph memory updater: simulation_id={}",
                simulation_id
            );
        }
    }

    /// Stop all updaters. Idempotent -- subsequent calls are no-ops.
    pub async fn stop_all(&self) {
        {
            let done = *self.stop_all_done.read().await;
            if done {
                return;
            }
        }
        {
            let mut done = self.stop_all_done.write().await;
            *done = true;
        }

        let mut map = self.updaters.lock().await;
        for (sim_id, mut updater) in map.drain() {
            if let Err(e) = tokio::time::timeout(Duration::from_secs(10), updater.stop()).await {
                error!(
                    "Failed to stop updater: simulation_id={}, error={}",
                    sim_id, e
                );
            }
        }
        info!("Stopped all graph memory updaters");
    }

    /// Gather statistics from all active updaters.
    pub async fn get_all_stats(&self) -> HashMap<String, UpdaterStats> {
        let map = self.updaters.lock().await;
        let mut result = HashMap::new();
        for (sim_id, updater) in map.iter() {
            result.insert(sim_id.clone(), updater.get_stats().await);
        }
        result
    }
}

impl Default for ZepGraphMemoryManager {
    fn default() -> Self {
        Self::new()
    }
}
