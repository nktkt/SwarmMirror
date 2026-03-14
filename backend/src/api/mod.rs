pub mod graph;
pub mod simulation;
pub mod report;

use std::sync::Arc;
use crate::config::Config;
use crate::models::task::TaskManager;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub task_manager: TaskManager,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            task_manager: TaskManager::new(),
        }
    }
}
