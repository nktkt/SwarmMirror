use serde_json::{json, Value};
use tokio::fs;

pub struct SimulationRunner;

impl SimulationRunner {
    /// Run simulation (in Rust we simulate the behavior since OASIS is a Python library)
    /// In production, this would call the Python OASIS subprocess or implement the simulation logic natively
    pub async fn run_simulation(
        sim_dir: &std::path::Path,
        max_rounds: usize,
        enable_twitter: bool,
        _enable_reddit: bool,
        progress_callback: Option<Box<dyn Fn(usize, &str) + Send>>,
    ) -> anyhow::Result<Value> {
        // Load profiles
        let profiles_path = sim_dir.join("reddit_profiles.json");
        let profiles: Vec<Value> = if profiles_path.exists() {
            let data = fs::read_to_string(&profiles_path).await?;
            serde_json::from_str(&data)?
        } else {
            vec![]
        };

        // Load config
        let config_path = sim_dir.join("simulation_config.json");
        let _config: Value = if config_path.exists() {
            let data = fs::read_to_string(&config_path).await?;
            serde_json::from_str(&data)?
        } else {
            json!({})
        };

        let mut all_actions = Vec::new();

        for round in 1..=max_rounds {
            if let Some(ref cb) = progress_callback {
                cb(round, &format!("Round {}/{}", round, max_rounds));
            }

            // Generate actions for each agent in this round
            let mut round_actions = Vec::new();
            for profile in &profiles {
                let username = profile["username"].as_str().unwrap_or("unknown");
                let action = json!({
                    "round": round,
                    "agent": username,
                    "platform": if enable_twitter { "twitter" } else { "reddit" },
                    "action_type": "CREATE_POST",
                    "content": format!("Simulated post by {} in round {}", username, round),
                    "timestamp": chrono::Local::now().to_rfc3339(),
                });
                round_actions.push(action);
            }

            // Save round log
            let round_log_path = sim_dir.join(format!("round_{:03}.json", round));
            let log_json = serde_json::to_string_pretty(&round_actions)?;
            fs::write(&round_log_path, &log_json).await?;

            all_actions.extend(round_actions);

            // Small delay between rounds
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Ok(json!({
            "total_rounds": max_rounds,
            "total_actions": all_actions.len(),
            "agents_count": profiles.len(),
        }))
    }
}
