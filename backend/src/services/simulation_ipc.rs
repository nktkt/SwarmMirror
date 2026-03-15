//! Simulation IPC communication module.
//!
//! Handles inter-process communication between the backend and simulation
//! scripts using a file-system based command/response pattern:
//! 1. Backend writes commands to `ipc_commands/` directory
//! 2. Simulation script polls the commands directory, executes commands,
//!    and writes responses to `ipc_responses/` directory
//! 3. Backend polls the responses directory for results

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::fs;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Command types for IPC messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandType {
    /// Interview a single agent
    Interview,
    /// Batch interview multiple agents
    BatchInterview,
    /// Close the simulation environment
    CloseEnv,
}

impl std::fmt::Display for CommandType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandType::Interview => write!(f, "interview"),
            CommandType::BatchInterview => write!(f, "batch_interview"),
            CommandType::CloseEnv => write!(f, "close_env"),
        }
    }
}

/// Status of an IPC command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl std::fmt::Display for CommandStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandStatus::Pending => write!(f, "pending"),
            CommandStatus::Processing => write!(f, "processing"),
            CommandStatus::Completed => write!(f, "completed"),
            CommandStatus::Failed => write!(f, "failed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// An IPC command sent from the backend to the simulation process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IPCCommand {
    pub command_id: String,
    pub command_type: CommandType,
    pub args: HashMap<String, Value>,
    pub timestamp: String,
}

impl IPCCommand {
    /// Create a new IPC command with the current timestamp.
    pub fn new(command_type: CommandType, args: HashMap<String, Value>) -> Self {
        Self {
            command_id: Uuid::new_v4().to_string(),
            command_type,
            args,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Serialize to a JSON `Value`.
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// Deserialize from a JSON `Value`.
    pub fn from_dict(data: &Value) -> Result<Self> {
        let cmd: IPCCommand = serde_json::from_value(data.clone())?;
        Ok(cmd)
    }
}

/// A response to an IPC command sent from the simulation process back to the
/// backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IPCResponse {
    pub command_id: String,
    pub status: CommandStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub timestamp: String,
}

impl IPCResponse {
    /// Create a new response with the current timestamp.
    pub fn new(
        command_id: String,
        status: CommandStatus,
        result: Option<HashMap<String, Value>>,
        error: Option<String>,
    ) -> Self {
        Self {
            command_id,
            status,
            result,
            error,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Serialize to a JSON `Value`.
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// Deserialize from a JSON `Value`.
    pub fn from_dict(data: &Value) -> Result<Self> {
        let resp: IPCResponse = serde_json::from_value(data.clone())?;
        Ok(resp)
    }
}

// ---------------------------------------------------------------------------
// SimulationIPCClient (backend / Flask side)
// ---------------------------------------------------------------------------

/// IPC client used by the backend to send commands to the simulation process
/// and wait for responses.
pub struct SimulationIPCClient {
    pub simulation_dir: PathBuf,
    pub commands_dir: PathBuf,
    pub responses_dir: PathBuf,
}

impl SimulationIPCClient {
    /// Create a new IPC client.
    ///
    /// Ensures that the `ipc_commands` and `ipc_responses` directories exist
    /// inside `simulation_dir`.
    pub async fn new(simulation_dir: impl Into<PathBuf>) -> Result<Self> {
        let simulation_dir = simulation_dir.into();
        let commands_dir = simulation_dir.join("ipc_commands");
        let responses_dir = simulation_dir.join("ipc_responses");

        fs::create_dir_all(&commands_dir).await?;
        fs::create_dir_all(&responses_dir).await?;

        Ok(Self {
            simulation_dir,
            commands_dir,
            responses_dir,
        })
    }

    /// Send a command and wait for the response (with polling).
    ///
    /// # Arguments
    /// * `command_type` - The type of command to send.
    /// * `args` - Arguments for the command.
    /// * `timeout_secs` - Maximum time to wait for a response (seconds).
    /// * `poll_interval_secs` - Polling interval (seconds).
    ///
    /// # Errors
    /// Returns an error if the response is not received within `timeout_secs`.
    pub async fn send_command(
        &self,
        command_type: CommandType,
        args: HashMap<String, Value>,
        timeout_secs: f64,
        poll_interval_secs: f64,
    ) -> Result<IPCResponse> {
        let command = IPCCommand::new(command_type.clone(), args);
        let command_id = command.command_id.clone();

        // Write the command file
        let command_file = self.commands_dir.join(format!("{}.json", command_id));
        let json_bytes = serde_json::to_string_pretty(&command)?;
        fs::write(&command_file, json_bytes).await?;

        info!(
            "Sent IPC command: {}, command_id={}",
            command_type, command_id
        );

        // Poll for response
        let response_file = self.responses_dir.join(format!("{}.json", command_id));
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs_f64(timeout_secs);
        let poll_interval = Duration::from_secs_f64(poll_interval_secs);

        while start.elapsed() < timeout {
            if response_file.exists() {
                match fs::read_to_string(&response_file).await {
                    Ok(content) => match serde_json::from_str::<IPCResponse>(&content) {
                        Ok(response) => {
                            // Clean up command and response files
                            let _ = fs::remove_file(&command_file).await;
                            let _ = fs::remove_file(&response_file).await;

                            info!(
                                "Received IPC response: command_id={}, status={}",
                                command_id, response.status
                            );
                            return Ok(response);
                        }
                        Err(e) => {
                            warn!("Failed to parse response: {}", e);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to read response file: {}", e);
                    }
                }
            }
            sleep(poll_interval).await;
        }

        // Timeout - clean up the command file
        error!("Timeout waiting for IPC response: command_id={}", command_id);
        let _ = fs::remove_file(&command_file).await;

        anyhow::bail!(
            "Timeout waiting for command response ({:.0}s)",
            timeout_secs
        )
    }

    /// Send a single-agent interview command.
    ///
    /// # Arguments
    /// * `agent_id` - The agent ID.
    /// * `prompt` - The interview question.
    /// * `platform` - Optional platform filter (`"twitter"` or `"reddit"`).
    /// * `timeout_secs` - Maximum wait time (default 60s).
    pub async fn send_interview(
        &self,
        agent_id: i64,
        prompt: &str,
        platform: Option<&str>,
        timeout_secs: f64,
    ) -> Result<IPCResponse> {
        let mut args = HashMap::new();
        args.insert("agent_id".into(), Value::from(agent_id));
        args.insert("prompt".into(), Value::from(prompt));
        if let Some(p) = platform {
            args.insert("platform".into(), Value::from(p));
        }

        self.send_command(CommandType::Interview, args, timeout_secs, 0.5)
            .await
    }

    /// Send a batch interview command.
    ///
    /// # Arguments
    /// * `interviews` - List of interview specs, each containing `agent_id`,
    ///   `prompt`, and optionally `platform`.
    /// * `platform` - Default platform filter.
    /// * `timeout_secs` - Maximum wait time (default 120s).
    pub async fn send_batch_interview(
        &self,
        interviews: Vec<Value>,
        platform: Option<&str>,
        timeout_secs: f64,
    ) -> Result<IPCResponse> {
        let mut args = HashMap::new();
        args.insert("interviews".into(), Value::from(interviews));
        if let Some(p) = platform {
            args.insert("platform".into(), Value::from(p));
        }

        self.send_command(CommandType::BatchInterview, args, timeout_secs, 0.5)
            .await
    }

    /// Send a close-environment command.
    ///
    /// # Arguments
    /// * `timeout_secs` - Maximum wait time (default 30s).
    pub async fn send_close_env(&self, timeout_secs: f64) -> Result<IPCResponse> {
        self.send_command(CommandType::CloseEnv, HashMap::new(), timeout_secs, 0.5)
            .await
    }

    /// Check whether the simulation environment is alive by reading
    /// `env_status.json`.
    pub async fn check_env_alive(&self) -> bool {
        let status_file = self.simulation_dir.join("env_status.json");
        if !status_file.exists() {
            return false;
        }

        match fs::read_to_string(&status_file).await {
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(v) => v.get("status").and_then(|s| s.as_str()) == Some("alive"),
                Err(_) => false,
            },
            Err(_) => false,
        }
    }
}

// ---------------------------------------------------------------------------
// SimulationIPCServer (simulation script side)
// ---------------------------------------------------------------------------

/// IPC server used by the simulation script to poll for commands, execute
/// them, and return responses.
pub struct SimulationIPCServer {
    pub simulation_dir: PathBuf,
    pub commands_dir: PathBuf,
    pub responses_dir: PathBuf,
    running: bool,
}

impl SimulationIPCServer {
    /// Create a new IPC server.
    ///
    /// Ensures that the `ipc_commands` and `ipc_responses` directories exist
    /// inside `simulation_dir`.
    pub async fn new(simulation_dir: impl Into<PathBuf>) -> Result<Self> {
        let simulation_dir = simulation_dir.into();
        let commands_dir = simulation_dir.join("ipc_commands");
        let responses_dir = simulation_dir.join("ipc_responses");

        fs::create_dir_all(&commands_dir).await?;
        fs::create_dir_all(&responses_dir).await?;

        Ok(Self {
            simulation_dir,
            commands_dir,
            responses_dir,
            running: false,
        })
    }

    /// Mark the server as running and update the environment status file.
    pub async fn start(&mut self) -> Result<()> {
        self.running = true;
        self.update_env_status("alive").await
    }

    /// Mark the server as stopped and update the environment status file.
    pub async fn stop(&mut self) -> Result<()> {
        self.running = false;
        self.update_env_status("stopped").await
    }

    /// Whether the server is currently running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Update the `env_status.json` file.
    async fn update_env_status(&self, status: &str) -> Result<()> {
        let status_file = self.simulation_dir.join("env_status.json");
        let data = serde_json::json!({
            "status": status,
            "timestamp": Utc::now().to_rfc3339(),
        });
        let json_bytes = serde_json::to_string_pretty(&data)?;
        fs::write(&status_file, json_bytes).await?;
        Ok(())
    }

    /// Poll the commands directory and return the first pending command
    /// (sorted by modification time).
    pub async fn poll_commands(&self) -> Option<IPCCommand> {
        if !self.commands_dir.exists() {
            return None;
        }

        // Collect command files with their modification times
        let mut entries: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

        let mut dir = match fs::read_dir(&self.commands_dir).await {
            Ok(d) => d,
            Err(_) => return None,
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(meta) = entry.metadata().await {
                    if let Ok(mtime) = meta.modified() {
                        entries.push((path, mtime));
                    }
                }
            }
        }

        // Sort by modification time (oldest first)
        entries.sort_by_key(|(_, mtime)| *mtime);

        for (filepath, _) in entries {
            match fs::read_to_string(&filepath).await {
                Ok(content) => match serde_json::from_str::<Value>(&content) {
                    Ok(data) => match IPCCommand::from_dict(&data) {
                        Ok(cmd) => return Some(cmd),
                        Err(e) => {
                            warn!("Failed to parse command file {:?}: {}", filepath, e);
                            continue;
                        }
                    },
                    Err(e) => {
                        warn!("Failed to parse JSON in {:?}: {}", filepath, e);
                        continue;
                    }
                },
                Err(e) => {
                    warn!("Failed to read command file {:?}: {}", filepath, e);
                    continue;
                }
            }
        }

        None
    }

    /// Send a response for a previously received command.
    pub async fn send_response(&self, response: &IPCResponse) -> Result<()> {
        let response_file = self
            .responses_dir
            .join(format!("{}.json", response.command_id));
        let json_bytes = serde_json::to_string_pretty(response)?;
        fs::write(&response_file, json_bytes).await?;

        // Delete the command file
        let command_file = self
            .commands_dir
            .join(format!("{}.json", response.command_id));
        let _ = fs::remove_file(&command_file).await;

        Ok(())
    }

    /// Send a success response.
    pub async fn send_success(
        &self,
        command_id: &str,
        result: HashMap<String, Value>,
    ) -> Result<()> {
        let response = IPCResponse::new(
            command_id.to_string(),
            CommandStatus::Completed,
            Some(result),
            None,
        );
        self.send_response(&response).await
    }

    /// Send an error response.
    pub async fn send_error(&self, command_id: &str, error: &str) -> Result<()> {
        let response = IPCResponse::new(
            command_id.to_string(),
            CommandStatus::Failed,
            None,
            Some(error.to_string()),
        );
        self.send_response(&response).await
    }
}
