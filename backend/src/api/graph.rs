use axum::{
    extract::{Multipart, Path, Query, State},
    routing::{get, post, delete},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::AppState;
use crate::error::{AppError, AppResult};
use crate::models::project::{ProjectManager, ProjectStatus, FileInfo};
use crate::models::task::TaskStatus;
use crate::services::file_parser::FileParser;
use crate::services::text_processor::TextProcessor;
use crate::services::llm_client::LLMClient;
use crate::services::ontology_generator::OntologyGenerator;
use crate::services::graph_builder::GraphBuilderService;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/project/{id}", get(get_project))
        .route("/project/list", get(list_projects))
        .route("/project/{id}", delete(delete_project))
        .route("/project/{id}/reset", post(reset_project))
        .route("/ontology/generate", post(generate_ontology))
        .route("/build", post(build_graph))
        .route("/task/{id}", get(get_task))
        .route("/tasks", get(list_tasks))
        .route("/data/{graph_id}", get(get_graph_data))
        .route("/delete/{graph_id}", delete(delete_graph))
}

// GET /project/:id
async fn get_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> AppResult<Json<Value>> {
    let pm = ProjectManager::new(&state.config.upload_folder);
    match pm.get_project(&project_id).await {
        Some(project) => Ok(Json(json!({
            "success": true,
            "data": project
        }))),
        None => Err(AppError::NotFound(format!("Project {} not found", project_id))),
    }
}

// GET /project/list
#[derive(Deserialize)]
struct ListProjectsQuery {
    limit: Option<usize>,
}

async fn list_projects(
    State(state): State<AppState>,
    Query(params): Query<ListProjectsQuery>,
) -> AppResult<Json<Value>> {
    let pm = ProjectManager::new(&state.config.upload_folder);
    let limit = params.limit.unwrap_or(50);
    let projects = pm.list_projects(limit).await;
    Ok(Json(json!({
        "success": true,
        "data": {
            "projects": projects,
            "total": projects.len()
        }
    })))
}

// DELETE /project/:id
async fn delete_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> AppResult<Json<Value>> {
    let pm = ProjectManager::new(&state.config.upload_folder);
    if pm.delete_project(&project_id).await {
        Ok(Json(json!({
            "success": true,
            "message": format!("Project {} deleted", project_id)
        })))
    } else {
        Err(AppError::NotFound(format!("Project {} not found", project_id)))
    }
}

// POST /project/:id/reset
async fn reset_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> AppResult<Json<Value>> {
    let pm = ProjectManager::new(&state.config.upload_folder);
    match pm.get_project(&project_id).await {
        Some(mut project) => {
            project.status = ProjectStatus::Created;
            project.ontology = None;
            project.analysis_summary = None;
            project.graph_id = None;
            project.graph_build_task_id = None;
            project.error = None;
            pm.save_project(&project).await.map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(Json(json!({
                "success": true,
                "data": project
            })))
        }
        None => Err(AppError::NotFound(format!("Project {} not found", project_id))),
    }
}

// POST /ontology/generate (multipart)
async fn generate_ontology(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Json<Value>> {
    let mut project_name = String::new();
    let mut simulation_requirement = String::new();
    let mut additional_context: Option<String> = None;
    let mut chunk_size: Option<usize> = None;
    let mut chunk_overlap: Option<usize> = None;
    let mut files_data: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "project_name" => {
                project_name = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
            }
            "simulation_requirement" => {
                simulation_requirement = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
            }
            "additional_context" => {
                let text = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                if !text.is_empty() {
                    additional_context = Some(text);
                }
            }
            "chunk_size" => {
                let text = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                chunk_size = text.parse().ok();
            }
            "chunk_overlap" => {
                let text = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                chunk_overlap = text.parse().ok();
            }
            "files" | "file" => {
                let filename = field.file_name().unwrap_or("unknown").to_string();
                let data = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                files_data.push((filename, data.to_vec()));
            }
            _ => {
                // Skip unknown fields
                let _ = field.bytes().await;
            }
        }
    }

    if project_name.is_empty() {
        return Err(AppError::BadRequest("project_name is required".into()));
    }
    if simulation_requirement.is_empty() {
        return Err(AppError::BadRequest("simulation_requirement is required".into()));
    }

    let pm = ProjectManager::new(&state.config.upload_folder);
    let mut project = pm.create_project(&project_name).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    project.simulation_requirement = Some(simulation_requirement.clone());
    project.chunk_size = chunk_size.unwrap_or(state.config.default_chunk_size);
    project.chunk_overlap = chunk_overlap.unwrap_or(state.config.default_chunk_overlap);

    // Save and extract text from uploaded files
    let mut all_texts = Vec::new();
    let mut file_infos = Vec::new();

    for (filename, data) in &files_data {
        if !state.config.is_allowed_extension(filename) {
            continue;
        }
        let (path, size) = pm.save_file(&project.project_id, filename, data).await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        file_infos.push(FileInfo {
            filename: filename.clone(),
            size,
        });

        match FileParser::extract_text(&path).await {
            Ok(text) => {
                let processed = TextProcessor::preprocess_text(&text);
                all_texts.push(processed);
            }
            Err(e) => {
                tracing::warn!("Failed to extract text from {}: {}", filename, e);
            }
        }
    }

    project.files = file_infos;
    let combined_text = all_texts.join("\n\n");
    project.total_text_length = combined_text.len();

    // Save extracted text
    if !combined_text.is_empty() {
        pm.save_extracted_text(&project.project_id, &combined_text).await.ok();
    }

    // Generate ontology using LLM
    let llm = LLMClient::new(
        &state.config.llm_api_key,
        &state.config.llm_base_url,
        &state.config.llm_model_name,
    );
    let generator = OntologyGenerator::new(llm);

    match generator.generate(
        &all_texts,
        &simulation_requirement,
        additional_context.as_deref(),
    ).await {
        Ok(ontology) => {
            project.ontology = Some(ontology.clone());
            project.analysis_summary = ontology["analysis_summary"].as_str().map(|s| s.to_string());
            project.status = ProjectStatus::OntologyGenerated;
            pm.save_project(&project).await.map_err(|e| AppError::Internal(e.to_string()))?;

            Ok(Json(json!({
                "success": true,
                "data": {
                    "project": project,
                    "ontology": ontology
                }
            })))
        }
        Err(e) => {
            project.status = ProjectStatus::Failed;
            project.error = Some(e.to_string());
            pm.save_project(&project).await.ok();
            Err(AppError::Internal(format!("Ontology generation failed: {}", e)))
        }
    }
}

// POST /build
#[derive(Deserialize)]
struct BuildRequest {
    project_id: String,
    #[serde(default)]
    ontology: Option<Value>,
    #[serde(default)]
    batch_size: Option<usize>,
}

async fn build_graph(
    State(state): State<AppState>,
    Json(body): Json<BuildRequest>,
) -> AppResult<Json<Value>> {
    let pm = ProjectManager::new(&state.config.upload_folder);
    let mut project = pm.get_project(&body.project_id).await
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", body.project_id)))?;

    // Get ontology
    let ontology = body.ontology.or(project.ontology.clone())
        .ok_or_else(|| AppError::BadRequest("No ontology available. Generate ontology first.".into()))?;

    // Create task
    let task_id = state.task_manager.create_task("graph_build", Some(json!({
        "project_id": project.project_id,
    })));

    project.status = ProjectStatus::GraphBuilding;
    project.graph_build_task_id = Some(task_id.clone());
    pm.save_project(&project).await.map_err(|e| AppError::Internal(e.to_string()))?;

    // Spawn background task
    let config = state.config.clone();
    let task_manager = state.task_manager.clone();
    let project_id = project.project_id.clone();
    let project_name = project.name.clone();
    let batch_size = body.batch_size.unwrap_or(5);
    let chunk_size = project.chunk_size;
    let chunk_overlap = project.chunk_overlap;
    let upload_folder = config.upload_folder.clone();
    let task_id_ret = task_id.clone();

    tokio::spawn(async move {
        let pm = ProjectManager::new(&upload_folder);

        task_manager.update_task(
            &task_id, Some(TaskStatus::Processing), Some(5),
            Some("Starting graph build..."), None, None,
        );

        // Get extracted text
        let text = match pm.get_extracted_text(&project_id).await {
            Some(t) => t,
            None => {
                task_manager.fail_task(&task_id, "No extracted text found");
                return;
            }
        };

        // Split text into chunks
        let chunks = TextProcessor::split_text(&text, chunk_size, chunk_overlap);
        task_manager.update_task(
            &task_id, None, Some(10),
            Some(&format!("Text split into {} chunks", chunks.len())), None, None,
        );

        // Build graph
        let graph_service = GraphBuilderService::new(&config.zep_api_key);

        // Create graph
        let graph_id = match graph_service.create_graph(&project_name).await {
            Ok(id) => id,
            Err(e) => {
                task_manager.fail_task(&task_id, &format!("Failed to create graph: {}", e));
                if let Some(mut proj) = pm.get_project(&project_id).await {
                    proj.status = ProjectStatus::Failed;
                    proj.error = Some(e.to_string());
                    pm.save_project(&proj).await.ok();
                }
                return;
            }
        };

        task_manager.update_task(
            &task_id, None, Some(15),
            Some(&format!("Graph created: {}", graph_id)), None, None,
        );

        // Set ontology
        if let Err(e) = graph_service.set_ontology(&graph_id, &ontology).await {
            task_manager.fail_task(&task_id, &format!("Failed to set ontology: {}", e));
            return;
        }

        task_manager.update_task(
            &task_id, None, Some(20),
            Some("Ontology configured"), None, None,
        );

        // Add text in batches
        let episode_uuids = match graph_service.add_text_batches(
            &graph_id, &chunks, batch_size, None,
        ).await {
            Ok(uuids) => uuids,
            Err(e) => {
                task_manager.fail_task(&task_id, &format!("Failed to add text: {}", e));
                return;
            }
        };

        task_manager.update_task(
            &task_id, None, Some(50),
            Some(&format!("Submitted {} episodes, waiting for processing...", episode_uuids.len())), None, None,
        );

        // Wait for processing
        graph_service.wait_for_episodes(&episode_uuids, None).await;

        task_manager.update_task(
            &task_id, None, Some(80),
            Some("Episodes processed, fetching graph data..."), None, None,
        );

        // Get graph data
        let graph_data = match graph_service.get_graph_data(&graph_id).await {
            Ok(data) => data,
            Err(e) => {
                task_manager.fail_task(&task_id, &format!("Failed to fetch graph data: {}", e));
                return;
            }
        };

        // Update project
        if let Some(mut proj) = pm.get_project(&project_id).await {
            proj.graph_id = Some(graph_id.clone());
            proj.status = ProjectStatus::GraphCompleted;
            pm.save_project(&proj).await.ok();
        }

        task_manager.complete_task(&task_id, json!({
            "graph_id": graph_id,
            "node_count": graph_data["node_count"],
            "edge_count": graph_data["edge_count"],
        }));
    });

    Ok(Json(json!({
        "success": true,
        "data": {
            "task_id": task_id_ret,
            "project_id": project.project_id,
            "message": "Graph build started"
        }
    })))
}

// GET /task/:id
async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> AppResult<Json<Value>> {
    match state.task_manager.get_task(&task_id) {
        Some(task) => Ok(Json(json!({
            "success": true,
            "data": task
        }))),
        None => Err(AppError::NotFound(format!("Task {} not found", task_id))),
    }
}

// GET /tasks
async fn list_tasks(
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let tasks = state.task_manager.list_tasks();
    Ok(Json(json!({
        "success": true,
        "data": {
            "tasks": tasks,
            "total": tasks.len()
        }
    })))
}

// GET /data/:graph_id
async fn get_graph_data(
    State(state): State<AppState>,
    Path(graph_id): Path<String>,
) -> AppResult<Json<Value>> {
    let graph_service = GraphBuilderService::new(&state.config.zep_api_key);
    match graph_service.get_graph_data(&graph_id).await {
        Ok(data) => Ok(Json(json!({
            "success": true,
            "data": data
        }))),
        Err(e) => Err(AppError::Internal(format!("Failed to get graph data: {}", e))),
    }
}

// DELETE /delete/:graph_id
async fn delete_graph(
    State(state): State<AppState>,
    Path(graph_id): Path<String>,
) -> AppResult<Json<Value>> {
    let graph_service = GraphBuilderService::new(&state.config.zep_api_key);
    match graph_service.delete_graph(&graph_id).await {
        Ok(_) => Ok(Json(json!({
            "success": true,
            "message": format!("Graph {} deleted", graph_id)
        }))),
        Err(e) => Err(AppError::Internal(format!("Failed to delete graph: {}", e))),
    }
}
