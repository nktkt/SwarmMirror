use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use chrono::Local;
use uuid::Uuid;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Created,
    OntologyGenerated,
    GraphBuilding,
    GraphCompleted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub project_id: String,
    pub name: String,
    pub status: ProjectStatus,
    pub created_at: String,
    pub updated_at: String,
    pub files: Vec<FileInfo>,
    pub total_text_length: usize,
    pub ontology: Option<serde_json::Value>,
    pub analysis_summary: Option<String>,
    pub graph_id: Option<String>,
    pub graph_build_task_id: Option<String>,
    pub simulation_requirement: Option<String>,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub error: Option<String>,
}

impl Project {
    pub fn new(name: &str) -> Self {
        let now = Local::now().to_rfc3339();
        Self {
            project_id: format!("proj_{}", &Uuid::new_v4().to_string().replace('-', "")[..12]),
            name: name.to_string(),
            status: ProjectStatus::Created,
            created_at: now.clone(),
            updated_at: now,
            files: Vec::new(),
            total_text_length: 0,
            ontology: None,
            analysis_summary: None,
            graph_id: None,
            graph_build_task_id: None,
            simulation_requirement: None,
            chunk_size: 500,
            chunk_overlap: 50,
            error: None,
        }
    }
}

pub struct ProjectManager {
    projects_dir: PathBuf,
}

impl ProjectManager {
    pub fn new(upload_folder: &Path) -> Self {
        let projects_dir = upload_folder.join("projects");
        Self { projects_dir }
    }

    async fn ensure_dir(&self) {
        fs::create_dir_all(&self.projects_dir).await.ok();
    }

    fn project_dir(&self, project_id: &str) -> PathBuf {
        self.projects_dir.join(project_id)
    }

    fn meta_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("project.json")
    }

    fn files_dir(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("files")
    }

    fn text_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("extracted_text.txt")
    }

    pub async fn create_project(&self, name: &str) -> anyhow::Result<Project> {
        self.ensure_dir().await;
        let project = Project::new(name);
        let proj_dir = self.project_dir(&project.project_id);
        let files_dir = self.files_dir(&project.project_id);
        fs::create_dir_all(&proj_dir).await?;
        fs::create_dir_all(&files_dir).await?;
        self.save_project(&project).await?;
        Ok(project)
    }

    pub async fn save_project(&self, project: &Project) -> anyhow::Result<()> {
        let mut project = project.clone();
        project.updated_at = Local::now().to_rfc3339();
        let meta_path = self.meta_path(&project.project_id);
        let json = serde_json::to_string_pretty(&project)?;
        fs::write(&meta_path, json).await?;
        Ok(())
    }

    pub async fn get_project(&self, project_id: &str) -> Option<Project> {
        let meta_path = self.meta_path(project_id);
        let data = fs::read_to_string(&meta_path).await.ok()?;
        serde_json::from_str(&data).ok()
    }

    pub async fn list_projects(&self, limit: usize) -> Vec<Project> {
        self.ensure_dir().await;
        let mut projects = Vec::new();
        if let Ok(mut entries) = fs::read_dir(&self.projects_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                if let Some(p) = self.get_project(&name).await {
                    projects.push(p);
                }
            }
        }
        projects.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        projects.truncate(limit);
        projects
    }

    pub async fn delete_project(&self, project_id: &str) -> bool {
        let proj_dir = self.project_dir(project_id);
        if proj_dir.exists() {
            fs::remove_dir_all(&proj_dir).await.ok();
            true
        } else {
            false
        }
    }

    pub async fn save_file(&self, project_id: &str, filename: &str, data: &[u8]) -> anyhow::Result<(PathBuf, u64)> {
        let files_dir = self.files_dir(project_id);
        fs::create_dir_all(&files_dir).await?;
        let ext = Path::new(filename).extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let safe_name = format!("{}.{}", &Uuid::new_v4().to_string().replace('-', "")[..8], ext);
        let path = files_dir.join(&safe_name);
        fs::write(&path, data).await?;
        let size = data.len() as u64;
        Ok((path, size))
    }

    pub async fn save_extracted_text(&self, project_id: &str, text: &str) -> anyhow::Result<()> {
        let path = self.text_path(project_id);
        fs::write(&path, text).await?;
        Ok(())
    }

    pub async fn get_extracted_text(&self, project_id: &str) -> Option<String> {
        let path = self.text_path(project_id);
        fs::read_to_string(&path).await.ok()
    }
}
