use axum::{
    extract::{Path, Query, State},
    routing::{get, post, delete},
    Json, Router,
    response::IntoResponse,
    http::header,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::AppState;
use crate::error::{AppError, AppResult};
use crate::models::project::ProjectManager;
use crate::models::simulation::SimulationManager;
use crate::models::report::{ReportManager, ReportStatus};
use crate::models::task::TaskStatus;
use crate::services::llm_client::LLMClient;
use crate::services::zep_client::ZepClient;
use crate::services::report_agent::ReportAgent;
use crate::services::zep_tools::ZepToolsService;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/generate", post(generate_report))
        .route("/generate/status", post(generate_status))
        .route("/list", get(list_reports))
        .route("/check/{sim_id}", get(check_report))
        .route("/by-simulation/{sim_id}", get(get_report_by_simulation))
        .route("/tools/search", post(tools_search))
        .route("/tools/statistics", post(tools_statistics))
        .route("/{report_id}", get(get_report))
        .route("/{report_id}", delete(delete_report))
        .route("/{report_id}/download", get(download_report))
        .route("/{report_id}/progress", get(get_progress))
        .route("/{report_id}/sections", get(get_sections))
        .route("/{report_id}/section/{index}", get(get_section))
        .route("/{report_id}/agent-log", get(get_agent_log))
        .route("/{report_id}/agent-log/stream", get(get_agent_log_stream))
        .route("/{report_id}/console-log", get(get_console_log))
        .route("/{report_id}/console-log/stream", get(get_console_log_stream))
        .route("/chat", post(chat))
}

// POST /generate
#[derive(Deserialize)]
struct GenerateRequest {
    simulation_id: String,
    #[serde(default)]
    report_id: Option<String>,
}

async fn generate_report(
    State(state): State<AppState>,
    Json(body): Json<GenerateRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);

    let simulation = sim_manager.get_simulation(&body.simulation_id).await
        .ok_or_else(|| AppError::NotFound(format!("Simulation {} not found", body.simulation_id)))?;

    // Create task
    let task_id = state.task_manager.create_task("report_generate", Some(json!({
        "simulation_id": simulation.simulation_id,
    })));

    // Spawn background task
    let config = state.config.clone();
    let task_manager = state.task_manager.clone();
    let sim_id = simulation.simulation_id.clone();
    let graph_id = simulation.graph_id.clone();
    let project_id = simulation.project_id.clone();
    let report_id = body.report_id.clone();
    let task_id_ret = task_id.clone();

    tokio::spawn(async move {
        let report_manager = ReportManager::new(&config.simulation_data_dir);
        let pm = ProjectManager::new(&config.upload_folder);

        task_manager.update_task(
            &task_id, Some(TaskStatus::Processing), Some(10),
            Some("Starting report generation..."), None, None,
        );

        let simulation_requirement = pm.get_project(&project_id).await
            .and_then(|p| p.simulation_requirement)
            .unwrap_or_else(|| "General social simulation analysis".to_string());

        let llm = LLMClient::new(
            &config.llm_api_key,
            &config.llm_base_url,
            &config.llm_model_name,
        );
        let zep = ZepClient::new(&config.zep_api_key);
        let agent = ReportAgent::new(llm, zep, &graph_id, &sim_id, &simulation_requirement);

        let task_mgr_clone = task_manager.clone();
        let task_id_clone = task_id.clone();

        match agent.generate_report(
            report_id.as_deref(),
            Some(&move |stage: &str, progress: i32, message: &str| {
                task_mgr_clone.update_task(
                    &task_id_clone, None, Some(progress),
                    Some(&format!("[{}] {}", stage, message)), None, None,
                );
            }),
        ).await {
            Ok(report) => {
                let rid = report.report_id.clone();
                if let Err(e) = report_manager.save_report(&report).await {
                    task_manager.fail_task(&task_id, &format!("Failed to save report: {}", e));
                    return;
                }

                task_manager.complete_task(&task_id, json!({
                    "report_id": rid,
                    "simulation_id": sim_id,
                    "status": "completed",
                }));
            }
            Err(e) => {
                // Save failed report
                let mut report = crate::models::report::Report::new(&sim_id, report_id.as_deref());
                report.status = ReportStatus::Failed;
                report.error = Some(e.to_string());
                report_manager.save_report(&report).await.ok();

                task_manager.fail_task(&task_id, &e.to_string());
            }
        }
    });

    Ok(Json(json!({
        "success": true,
        "data": {
            "task_id": task_id_ret,
            "simulation_id": body.simulation_id,
            "message": "Report generation started"
        }
    })))
}

// POST /generate/status
#[derive(Deserialize)]
struct GenerateStatusRequest {
    task_id: String,
}

async fn generate_status(
    State(state): State<AppState>,
    Json(body): Json<GenerateStatusRequest>,
) -> AppResult<Json<Value>> {
    match state.task_manager.get_task(&body.task_id) {
        Some(task) => Ok(Json(json!({
            "success": true,
            "data": task
        }))),
        None => Err(AppError::NotFound(format!("Task {} not found", body.task_id))),
    }
}

// GET /:report_id
async fn get_report(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);

    match report_manager.get_report(&report_id).await {
        Some(report) => Ok(Json(json!({
            "success": true,
            "data": report
        }))),
        None => Err(AppError::NotFound(format!("Report {} not found", report_id))),
    }
}

// GET /by-simulation/:sim_id
async fn get_report_by_simulation(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);

    match report_manager.get_report_by_simulation(&sim_id).await {
        Some(report) => Ok(Json(json!({
            "success": true,
            "data": report
        }))),
        None => Err(AppError::NotFound(format!("No report found for simulation {}", sim_id))),
    }
}

// GET /list
#[derive(Deserialize)]
struct ListReportsQuery {
    simulation_id: Option<String>,
    limit: Option<usize>,
}

async fn list_reports(
    State(state): State<AppState>,
    Query(params): Query<ListReportsQuery>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);
    let limit = params.limit.unwrap_or(50);
    let reports = report_manager.list_reports(params.simulation_id.as_deref(), limit).await;

    Ok(Json(json!({
        "success": true,
        "data": {
            "reports": reports,
            "total": reports.len()
        }
    })))
}

// GET /:report_id/download
async fn download_report(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);

    let report = report_manager.get_report(&report_id).await
        .ok_or_else(|| AppError::NotFound(format!("Report {} not found", report_id)))?;

    let content = if report.markdown_content.is_empty() {
        report_manager.get_markdown_content(&report_id).await
            .unwrap_or_else(|| "No content available".to_string())
    } else {
        report.markdown_content
    };

    let filename = format!("report_{}.md", report_id);

    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename)),
        ],
        content,
    ))
}

// DELETE /:report_id
async fn delete_report(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);

    if report_manager.delete_report(&report_id).await {
        Ok(Json(json!({
            "success": true,
            "message": format!("Report {} deleted", report_id)
        })))
    } else {
        Err(AppError::NotFound(format!("Report {} not found", report_id)))
    }
}

// POST /chat
#[derive(Deserialize)]
struct ChatRequest {
    simulation_id: String,
    message: String,
    #[serde(default)]
    chat_history: Vec<Value>,
}

async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let pm = ProjectManager::new(&state.config.upload_folder);

    let simulation = sim_manager.get_simulation(&body.simulation_id).await
        .ok_or_else(|| AppError::NotFound(format!("Simulation {} not found", body.simulation_id)))?;

    let simulation_requirement = pm.get_project(&simulation.project_id).await
        .and_then(|p| p.simulation_requirement)
        .unwrap_or_else(|| "General social simulation".to_string());

    let llm = LLMClient::new(
        &state.config.llm_api_key,
        &state.config.llm_base_url,
        &state.config.llm_model_name,
    );
    let zep = ZepClient::new(&state.config.zep_api_key);
    let agent = ReportAgent::new(llm, zep, &simulation.graph_id, &body.simulation_id, &simulation_requirement);

    match agent.chat(&body.message, &body.chat_history).await {
        Ok(result) => Ok(Json(json!({
            "success": true,
            "data": result
        }))),
        Err(e) => Err(AppError::Internal(format!("Chat failed: {}", e))),
    }
}

// GET /:report_id/progress
async fn get_progress(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);

    match report_manager.get_progress(&report_id).await {
        Some(progress) => Ok(Json(json!({
            "success": true,
            "data": progress
        }))),
        None => Err(AppError::NotFound(format!("Report {} not found", report_id))),
    }
}

// GET /:report_id/sections
async fn get_sections(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);
    let sections = report_manager.get_sections(&report_id).await;

    Ok(Json(json!({
        "success": true,
        "data": {
            "sections": sections,
            "total": sections.len(),
            "report_id": report_id
        }
    })))
}

// GET /:report_id/section/:index
async fn get_section(
    State(state): State<AppState>,
    Path((report_id, index)): Path<(String, usize)>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);
    let sections = report_manager.get_sections(&report_id).await;

    let section = sections.iter()
        .find(|s| s["section_index"].as_u64() == Some(index as u64));

    match section {
        Some(s) => Ok(Json(json!({
            "success": true,
            "data": s
        }))),
        None => Err(AppError::NotFound(format!("Section {} not found in report {}", index, report_id))),
    }
}

// GET /check/:sim_id
async fn check_report(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);

    let report = report_manager.get_report_by_simulation(&sim_id).await;
    let exists = report.is_some();

    Ok(Json(json!({
        "success": true,
        "data": {
            "exists": exists,
            "report": report,
            "simulation_id": sim_id
        }
    })))
}

// GET /:report_id/agent-log
#[derive(Deserialize)]
struct LogQuery {
    from_line: Option<usize>,
}

async fn get_agent_log(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
    Query(params): Query<LogQuery>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);
    let from_line = params.from_line.unwrap_or(0);
    let logs = report_manager.get_agent_log(&report_id, from_line).await;

    Ok(Json(json!({
        "success": true,
        "data": logs
    })))
}

// GET /:report_id/agent-log/stream
async fn get_agent_log_stream(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
    Query(params): Query<LogQuery>,
) -> AppResult<Json<Value>> {
    // For now, return the same as the non-stream version
    // In production, this would be SSE (Server-Sent Events)
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);
    let from_line = params.from_line.unwrap_or(0);
    let logs = report_manager.get_agent_log(&report_id, from_line).await;

    Ok(Json(json!({
        "success": true,
        "data": logs
    })))
}

// GET /:report_id/console-log
async fn get_console_log(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
    Query(params): Query<LogQuery>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);
    let from_line = params.from_line.unwrap_or(0);
    let logs = report_manager.get_console_log(&report_id, from_line).await;

    Ok(Json(json!({
        "success": true,
        "data": logs
    })))
}

// GET /:report_id/console-log/stream
async fn get_console_log_stream(
    State(state): State<AppState>,
    Path(report_id): Path<String>,
    Query(params): Query<LogQuery>,
) -> AppResult<Json<Value>> {
    let report_manager = ReportManager::new(&state.config.simulation_data_dir);
    let from_line = params.from_line.unwrap_or(0);
    let logs = report_manager.get_console_log(&report_id, from_line).await;

    Ok(Json(json!({
        "success": true,
        "data": logs
    })))
}

// POST /tools/search
#[derive(Deserialize)]
struct ToolsSearchRequest {
    graph_id: String,
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

async fn tools_search(
    State(state): State<AppState>,
    Json(body): Json<ToolsSearchRequest>,
) -> AppResult<Json<Value>> {
    let tools = ZepToolsService::new(&state.config.zep_api_key);
    let limit = body.limit.unwrap_or(10);

    let result = tools.search_graph(&body.graph_id, &body.query, limit, "all").await;
    Ok(Json(json!({
        "success": true,
        "data": result.to_dict()
    })))
}

// POST /tools/statistics
#[derive(Deserialize)]
struct ToolsStatsRequest {
    graph_id: String,
}

async fn tools_statistics(
    State(state): State<AppState>,
    Json(body): Json<ToolsStatsRequest>,
) -> AppResult<Json<Value>> {
    let tools = ZepToolsService::new(&state.config.zep_api_key);

    let result = tools.get_graph_statistics(&body.graph_id).await;
    Ok(Json(json!({
        "success": true,
        "data": result
    })))
}
