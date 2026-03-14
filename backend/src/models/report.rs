use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use chrono::Local;
use uuid::Uuid;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Generating,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub report_id: String,
    pub simulation_id: String,
    pub status: ReportStatus,
    pub outline: Option<serde_json::Value>,
    pub markdown_content: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

impl Report {
    pub fn new(simulation_id: &str, report_id: Option<&str>) -> Self {
        let now = Local::now().to_rfc3339();
        Self {
            report_id: report_id.map(|s| s.to_string()).unwrap_or_else(|| {
                format!("report_{}", &Uuid::new_v4().to_string().replace('-', "")[..12])
            }),
            simulation_id: simulation_id.to_string(),
            status: ReportStatus::Generating,
            outline: None,
            markdown_content: String::new(),
            created_at: now,
            completed_at: None,
            error: None,
        }
    }
}

pub struct ReportManager {
    data_dir: PathBuf,
}

#[allow(dead_code)]
impl ReportManager {
    pub fn new(data_dir: &Path) -> Self {
        Self { data_dir: data_dir.join("reports") }
    }

    fn report_dir(&self, report_id: &str) -> PathBuf {
        self.data_dir.join(report_id)
    }

    fn meta_path(&self, report_id: &str) -> PathBuf {
        self.report_dir(report_id).join("report.json")
    }

    fn markdown_path(&self, report_id: &str) -> PathBuf {
        self.report_dir(report_id).join("report.md")
    }

    fn section_path(&self, report_id: &str, index: usize) -> PathBuf {
        self.report_dir(report_id).join(format!("section_{:02}.md", index))
    }

    fn agent_log_path(&self, report_id: &str) -> PathBuf {
        self.report_dir(report_id).join("agent_log.jsonl")
    }

    fn console_log_path(&self, report_id: &str) -> PathBuf {
        self.report_dir(report_id).join("console_log.txt")
    }

    pub async fn save_report(&self, report: &Report) -> anyhow::Result<()> {
        let dir = self.report_dir(&report.report_id);
        fs::create_dir_all(&dir).await?;
        let json = serde_json::to_string_pretty(report)?;
        fs::write(self.meta_path(&report.report_id), json).await?;
        if !report.markdown_content.is_empty() {
            fs::write(self.markdown_path(&report.report_id), &report.markdown_content).await?;
        }
        Ok(())
    }

    pub async fn get_report(&self, report_id: &str) -> Option<Report> {
        let path = self.meta_path(report_id);
        let data = fs::read_to_string(&path).await.ok()?;
        serde_json::from_str(&data).ok()
    }

    pub async fn get_report_by_simulation(&self, simulation_id: &str) -> Option<Report> {
        fs::create_dir_all(&self.data_dir).await.ok();
        if let Ok(mut entries) = fs::read_dir(&self.data_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                if let Some(report) = self.get_report(&name).await {
                    if report.simulation_id == simulation_id {
                        return Some(report);
                    }
                }
            }
        }
        None
    }

    pub async fn list_reports(&self, simulation_id: Option<&str>, limit: usize) -> Vec<Report> {
        let mut reports = Vec::new();
        fs::create_dir_all(&self.data_dir).await.ok();
        if let Ok(mut entries) = fs::read_dir(&self.data_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                if let Some(report) = self.get_report(&name).await {
                    if let Some(sid) = simulation_id {
                        if report.simulation_id != sid { continue; }
                    }
                    reports.push(report);
                }
            }
        }
        reports.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        reports.truncate(limit);
        reports
    }

    pub async fn delete_report(&self, report_id: &str) -> bool {
        let dir = self.report_dir(report_id);
        if dir.exists() {
            fs::remove_dir_all(&dir).await.ok();
            true
        } else {
            false
        }
    }

    pub async fn save_section(&self, report_id: &str, index: usize, content: &str) -> anyhow::Result<()> {
        let dir = self.report_dir(report_id);
        fs::create_dir_all(&dir).await?;
        let path = self.section_path(report_id, index);
        fs::write(&path, content).await?;
        Ok(())
    }

    pub async fn get_sections(&self, report_id: &str) -> Vec<serde_json::Value> {
        let dir = self.report_dir(report_id);
        let mut sections = Vec::new();
        if let Ok(mut entries) = fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("section_") && name.ends_with(".md") {
                    if let Ok(content) = fs::read_to_string(entry.path()).await {
                        let idx: usize = name.trim_start_matches("section_").trim_end_matches(".md")
                            .parse().unwrap_or(0);
                        sections.push(serde_json::json!({
                            "filename": name,
                            "section_index": idx,
                            "content": content
                        }));
                    }
                }
            }
        }
        sections.sort_by_key(|s| s["section_index"].as_u64().unwrap_or(0));
        sections
    }

    pub async fn get_progress(&self, report_id: &str) -> Option<serde_json::Value> {
        let report = self.get_report(report_id).await?;
        Some(serde_json::json!({
            "status": report.status,
            "report_id": report.report_id,
        }))
    }

    pub async fn get_agent_log(&self, report_id: &str, from_line: usize) -> serde_json::Value {
        let path = self.agent_log_path(report_id);
        let mut logs = Vec::new();
        if let Ok(content) = fs::read_to_string(&path).await {
            for line in content.lines().skip(from_line) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    logs.push(v);
                }
            }
        }
        let total = from_line + logs.len();
        serde_json::json!({
            "logs": logs,
            "total_lines": total,
            "from_line": from_line,
            "has_more": false
        })
    }

    pub async fn get_console_log(&self, report_id: &str, from_line: usize) -> serde_json::Value {
        let path = self.console_log_path(report_id);
        let mut logs = Vec::new();
        if let Ok(content) = fs::read_to_string(&path).await {
            for line in content.lines().skip(from_line) {
                logs.push(serde_json::Value::String(line.to_string()));
            }
        }
        let total = from_line + logs.len();
        serde_json::json!({
            "logs": logs,
            "total_lines": total,
            "from_line": from_line,
            "has_more": false
        })
    }

    pub async fn get_markdown_content(&self, report_id: &str) -> Option<String> {
        let path = self.markdown_path(report_id);
        fs::read_to_string(&path).await.ok()
    }
}
