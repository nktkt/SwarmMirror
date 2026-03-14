use serde::{Deserialize, Serialize};
use std::sync::Arc;
use chrono::Local;
use dashmap::DashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub task_type: String,
    pub status: TaskStatus,
    pub created_at: String,
    pub updated_at: String,
    pub progress: i32,
    pub message: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
    pub progress_detail: serde_json::Value,
}

#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<DashMap<String, Task>>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
        }
    }

    pub fn create_task(&self, task_type: &str, metadata: Option<serde_json::Value>) -> String {
        let task_id = Uuid::new_v4().to_string();
        let now = Local::now().to_rfc3339();
        let task = Task {
            task_id: task_id.clone(),
            task_type: task_type.to_string(),
            status: TaskStatus::Pending,
            created_at: now.clone(),
            updated_at: now,
            progress: 0,
            message: String::new(),
            result: None,
            error: None,
            metadata: metadata.unwrap_or(serde_json::Value::Null),
            progress_detail: serde_json::Value::Null,
        };
        self.tasks.insert(task_id.clone(), task);
        task_id
    }

    pub fn get_task(&self, task_id: &str) -> Option<Task> {
        self.tasks.get(task_id).map(|t| t.clone())
    }

    pub fn update_task(
        &self,
        task_id: &str,
        status: Option<TaskStatus>,
        progress: Option<i32>,
        message: Option<&str>,
        result: Option<serde_json::Value>,
        error: Option<&str>,
    ) {
        if let Some(mut task) = self.tasks.get_mut(task_id) {
            task.updated_at = Local::now().to_rfc3339();
            if let Some(s) = status { task.status = s; }
            if let Some(p) = progress { task.progress = p; }
            if let Some(m) = message { task.message = m.to_string(); }
            if let Some(r) = result { task.result = Some(r); }
            if let Some(e) = error { task.error = Some(e.to_string()); }
        }
    }

    pub fn complete_task(&self, task_id: &str, result: serde_json::Value) {
        self.update_task(task_id, Some(TaskStatus::Completed), Some(100), Some("Task completed"), Some(result), None);
    }

    pub fn fail_task(&self, task_id: &str, error: &str) {
        self.update_task(task_id, Some(TaskStatus::Failed), None, Some("Task failed"), None, Some(error));
    }

    pub fn list_tasks(&self) -> Vec<Task> {
        let mut tasks: Vec<Task> = self.tasks.iter().map(|t| t.value().clone()).collect();
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        tasks
    }
}
