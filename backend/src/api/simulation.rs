use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::AppState;
use crate::error::{AppError, AppResult};
use crate::models::project::ProjectManager;
use crate::models::simulation::{SimulationManager, SimulationStatus};
use crate::models::task::TaskStatus;
use crate::services::llm_client::LLMClient;
use crate::services::zep_entity_reader::ZepEntityReader;
use crate::services::profile_generator::ProfileGenerator;
use crate::services::simulation_config_generator::SimulationConfigGenerator;
use crate::services::simulation_runner::SimulationRunner;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/entities/{graph_id}", get(get_entities))
        .route("/entities/{graph_id}/{uuid}", get(get_entity))
        .route("/entities/{graph_id}/by-type/{entity_type}", get(get_entities_by_type))
        .route("/create", post(create_simulation))
        .route("/prepare", post(prepare_simulation))
        .route("/prepare/status", post(prepare_status))
        .route("/run", post(run_simulation))
        .route("/run/{sim_id}/status", get(run_status))
        .route("/run/{sim_id}/stop", post(stop_simulation))
        .route("/start", post(start_simulation))
        .route("/stop", post(stop_simulation_by_body))
        .route("/history", get(simulation_history))
        .route("/generate-profiles", post(generate_profiles_manual))
        .route("/interview", post(interview_agent))
        .route("/interview/batch", post(interview_batch))
        .route("/interview/all", post(interview_all))
        .route("/interview/history", post(interview_history))
        .route("/env-status", post(env_status))
        .route("/close-env", post(close_env))
        .route("/{sim_id}", get(get_simulation))
        .route("/list", get(list_simulations))
        .route("/{sim_id}/profiles", get(get_profiles))
        .route("/{sim_id}/profiles/realtime", get(get_profiles_realtime))
        .route("/{sim_id}/config", get(get_config))
        .route("/{sim_id}/config/realtime", get(get_config_realtime))
        .route("/{sim_id}/config/download", get(config_download))
        .route("/{sim_id}/run-status", get(run_status))
        .route("/{sim_id}/run-status/detail", get(run_status_detail))
        .route("/{sim_id}/actions", get(get_actions))
        .route("/{sim_id}/timeline", get(get_timeline))
        .route("/{sim_id}/agent-stats", get(get_agent_stats))
        .route("/{sim_id}/posts", get(get_posts))
        .route("/{sim_id}/comments", get(get_comments))
        .route("/script/{script_name}/download", get(download_script))
}

// GET /entities/:graph_id
#[derive(Deserialize)]
struct EntitiesQuery {
    enrich: Option<bool>,
}

async fn get_entities(
    State(state): State<AppState>,
    Path(graph_id): Path<String>,
    Query(params): Query<EntitiesQuery>,
) -> AppResult<Json<Value>> {
    let reader = ZepEntityReader::new(&state.config.zep_api_key);
    let enrich = params.enrich.unwrap_or(true);

    match reader.filter_defined_entities(&graph_id, None, enrich).await {
        Ok(result) => Ok(Json(json!({
            "success": true,
            "data": {
                "entities": result.entities,
                "entity_types": result.entity_types,
                "total_count": result.total_count,
                "filtered_count": result.filtered_count
            }
        }))),
        Err(e) => Err(AppError::Internal(format!("Failed to get entities: {}", e))),
    }
}

// GET /entities/:graph_id/:uuid
async fn get_entity(
    State(state): State<AppState>,
    Path((graph_id, uuid)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let reader = ZepEntityReader::new(&state.config.zep_api_key);

    match reader.get_entity_with_context(&graph_id, &uuid).await {
        Ok(Some(entity)) => Ok(Json(json!({
            "success": true,
            "data": entity
        }))),
        Ok(None) => Err(AppError::NotFound(format!("Entity {} not found", uuid))),
        Err(e) => Err(AppError::Internal(format!("Failed to get entity: {}", e))),
    }
}

// GET /entities/:graph_id/by-type/:entity_type
async fn get_entities_by_type(
    State(state): State<AppState>,
    Path((graph_id, entity_type)): Path<(String, String)>,
    Query(params): Query<EntitiesQuery>,
) -> AppResult<Json<Value>> {
    let reader = ZepEntityReader::new(&state.config.zep_api_key);
    let enrich = params.enrich.unwrap_or(true);
    let types = vec![entity_type.clone()];

    match reader.filter_defined_entities(&graph_id, Some(&types), enrich).await {
        Ok(result) => Ok(Json(json!({
            "success": true,
            "data": {
                "entities": result.entities,
                "entity_type": entity_type,
                "count": result.filtered_count
            }
        }))),
        Err(e) => Err(AppError::Internal(format!("Failed to get entities: {}", e))),
    }
}

// POST /create
#[derive(Deserialize)]
struct CreateSimulationRequest {
    project_id: String,
    graph_id: String,
    #[serde(default = "default_true")]
    enable_twitter: bool,
    #[serde(default = "default_true")]
    enable_reddit: bool,
}

fn default_true() -> bool { true }

async fn create_simulation(
    State(state): State<AppState>,
    Json(body): Json<CreateSimulationRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);

    match sim_manager.create_simulation(
        &body.project_id, &body.graph_id,
        body.enable_twitter, body.enable_reddit,
    ).await {
        Ok(simulation) => Ok(Json(json!({
            "success": true,
            "data": simulation
        }))),
        Err(e) => Err(AppError::Internal(format!("Failed to create simulation: {}", e))),
    }
}

// POST /prepare
#[derive(Deserialize)]
struct PrepareRequest {
    simulation_id: String,
    #[serde(default)]
    use_llm_profiles: Option<bool>,
    #[serde(default)]
    max_rounds: Option<usize>,
}

async fn prepare_simulation(
    State(state): State<AppState>,
    Json(body): Json<PrepareRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);

    let mut simulation = sim_manager.get_simulation(&body.simulation_id).await
        .ok_or_else(|| AppError::NotFound(format!("Simulation {} not found", body.simulation_id)))?;

    // Create task
    let task_id = state.task_manager.create_task("simulation_prepare", Some(json!({
        "simulation_id": simulation.simulation_id,
    })));

    simulation.status = SimulationStatus::Preparing;
    sim_manager.save_state(&simulation).await.map_err(|e| AppError::Internal(e.to_string()))?;

    // Spawn background task
    let config = state.config.clone();
    let task_manager = state.task_manager.clone();
    let sim_id = simulation.simulation_id.clone();
    let graph_id = simulation.graph_id.clone();
    let project_id = simulation.project_id.clone();
    let enable_twitter = simulation.enable_twitter;
    let enable_reddit = simulation.enable_reddit;
    let use_llm = body.use_llm_profiles.unwrap_or(true);
    let max_rounds = body.max_rounds.unwrap_or(config.oasis_default_max_rounds);
    let task_id_ret = task_id.clone();

    tokio::spawn(async move {
        let sim_manager = SimulationManager::new(&config.simulation_data_dir);
        let pm = ProjectManager::new(&config.upload_folder);

        task_manager.update_task(
            &task_id, Some(TaskStatus::Processing), Some(10),
            Some("Reading entities from graph..."), None, None,
        );

        // Read entities
        let reader = ZepEntityReader::new(&config.zep_api_key);
        let filtered = match reader.filter_defined_entities(&graph_id, None, true).await {
            Ok(f) => f,
            Err(e) => {
                task_manager.fail_task(&task_id, &format!("Failed to read entities: {}", e));
                return;
            }
        };

        task_manager.update_task(
            &task_id, None, Some(30),
            Some(&format!("Found {} entities, generating profiles...", filtered.entities.len())),
            None, None,
        );

        // Generate profiles
        let llm = LLMClient::new(
            &config.llm_api_key,
            &config.llm_base_url,
            &config.llm_model_name,
        );
        let profile_gen = ProfileGenerator::new(llm.clone());
        let profiles = match profile_gen.generate_profiles(&filtered.entities, use_llm).await {
            Ok(p) => p,
            Err(e) => {
                task_manager.fail_task(&task_id, &format!("Failed to generate profiles: {}", e));
                return;
            }
        };

        // Save profiles
        let sim_dir = sim_manager.sim_dir_path(&sim_id);
        let profiles_path = sim_dir.join("reddit_profiles.json");
        if let Err(e) = ProfileGenerator::save_profiles_json(&profiles, &profiles_path) {
            task_manager.fail_task(&task_id, &format!("Failed to save profiles: {}", e));
            return;
        }

        let csv_path = sim_dir.join("twitter_profiles.csv");
        ProfileGenerator::save_profiles_csv(&profiles, &csv_path).ok();

        task_manager.update_task(
            &task_id, None, Some(60),
            Some(&format!("{} profiles generated, creating simulation config...", profiles.len())),
            None, None,
        );

        // Generate simulation config
        let config_gen = SimulationConfigGenerator::new(llm);
        let doc_text = pm.get_extracted_text(&project_id).await.unwrap_or_default();

        let sim_config = match config_gen.generate_config(
            pm.get_project(&project_id).await
                .and_then(|p| p.simulation_requirement)
                .as_deref()
                .unwrap_or("General social simulation"),
            &doc_text,
            &filtered.entities,
            max_rounds,
            enable_twitter,
            enable_reddit,
        ).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Config generation failed, using defaults: {}", e);
                json!({
                    "max_rounds": max_rounds,
                    "simulation_topic": "Social simulation",
                    "initial_posts": [],
                    "agent_configs": [],
                    "generation_reasoning": "Default config (LLM generation failed)"
                })
            }
        };

        // Save config
        let config_path = sim_dir.join("simulation_config.json");
        if let Ok(json_str) = serde_json::to_string_pretty(&sim_config) {
            tokio::fs::write(&config_path, json_str).await.ok();
        }

        task_manager.update_task(
            &task_id, None, Some(90),
            Some("Configuration complete, finalizing..."), None, None,
        );

        // Update simulation state
        if let Some(mut sim) = sim_manager.get_simulation(&sim_id).await {
            sim.status = SimulationStatus::Ready;
            sim.entities_count = filtered.entities.len();
            sim.profiles_count = profiles.len();
            sim.entity_types = filtered.entity_types;
            sim.config_generated = true;
            sim.config_reasoning = sim_config["generation_reasoning"].as_str().unwrap_or("").to_string();
            sim_manager.save_state(&sim).await.ok();
        }

        task_manager.complete_task(&task_id, json!({
            "simulation_id": sim_id,
            "entities_count": filtered.filtered_count,
            "profiles_count": profiles.len(),
            "config_generated": true,
        }));
    });

    Ok(Json(json!({
        "success": true,
        "data": {
            "task_id": task_id_ret,
            "simulation_id": simulation.simulation_id,
            "message": "Simulation preparation started"
        }
    })))
}

// POST /prepare/status
#[derive(Deserialize)]
struct PrepareStatusRequest {
    task_id: String,
}

async fn prepare_status(
    State(state): State<AppState>,
    Json(body): Json<PrepareStatusRequest>,
) -> AppResult<Json<Value>> {
    match state.task_manager.get_task(&body.task_id) {
        Some(task) => Ok(Json(json!({
            "success": true,
            "data": task
        }))),
        None => Err(AppError::NotFound(format!("Task {} not found", body.task_id))),
    }
}

// POST /run
#[derive(Deserialize)]
struct RunRequest {
    simulation_id: String,
    #[serde(default)]
    max_rounds: Option<usize>,
}

async fn run_simulation(
    State(state): State<AppState>,
    Json(body): Json<RunRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);

    let mut simulation = sim_manager.get_simulation(&body.simulation_id).await
        .ok_or_else(|| AppError::NotFound(format!("Simulation {} not found", body.simulation_id)))?;

    if simulation.status != SimulationStatus::Ready && simulation.status != SimulationStatus::Completed {
        return Err(AppError::BadRequest("Simulation is not ready to run".into()));
    }

    let task_id = state.task_manager.create_task("simulation_run", Some(json!({
        "simulation_id": simulation.simulation_id,
    })));

    simulation.status = SimulationStatus::Running;
    sim_manager.save_state(&simulation).await.map_err(|e| AppError::Internal(e.to_string()))?;

    let config = state.config.clone();
    let task_manager = state.task_manager.clone();
    let sim_id = simulation.simulation_id.clone();
    let enable_twitter = simulation.enable_twitter;
    let enable_reddit = simulation.enable_reddit;
    let max_rounds = body.max_rounds.unwrap_or(config.oasis_default_max_rounds);
    let task_id_ret2 = task_id.clone();

    tokio::spawn(async move {
        let sim_manager = SimulationManager::new(&config.simulation_data_dir);
        let sim_dir = sim_manager.sim_dir_path(&sim_id);

        task_manager.update_task(
            &task_id, Some(TaskStatus::Processing), Some(5),
            Some("Starting simulation..."), None, None,
        );

        let task_mgr_clone = task_manager.clone();
        let task_id_clone = task_id.clone();

        let result = SimulationRunner::run_simulation(
            &sim_dir,
            max_rounds,
            enable_twitter,
            enable_reddit,
            Some(Box::new(move |round, msg| {
                let progress = (round as i32 * 90) / max_rounds as i32 + 5;
                task_mgr_clone.update_task(
                    &task_id_clone, None, Some(progress.min(95)),
                    Some(msg), None, None,
                );
            })),
        ).await;

        match result {
            Ok(run_result) => {
                if let Some(mut sim) = sim_manager.get_simulation(&sim_id).await {
                    sim.status = SimulationStatus::Completed;
                    sim.current_round = max_rounds;
                    if enable_twitter {
                        sim.twitter_status = "completed".to_string();
                    }
                    if enable_reddit {
                        sim.reddit_status = "completed".to_string();
                    }
                    sim_manager.save_state(&sim).await.ok();
                }

                task_manager.complete_task(&task_id, run_result);
            }
            Err(e) => {
                if let Some(mut sim) = sim_manager.get_simulation(&sim_id).await {
                    sim.status = SimulationStatus::Failed;
                    sim.error = Some(e.to_string());
                    sim_manager.save_state(&sim).await.ok();
                }
                task_manager.fail_task(&task_id, &e.to_string());
            }
        }
    });

    Ok(Json(json!({
        "success": true,
        "data": {
            "task_id": task_id_ret2,
            "simulation_id": simulation.simulation_id,
            "message": "Simulation started"
        }
    })))
}

// GET /run/:sim_id/status
async fn run_status(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);

    match sim_manager.get_simulation(&sim_id).await {
        Some(sim) => Ok(Json(json!({
            "success": true,
            "data": {
                "simulation_id": sim.simulation_id,
                "status": sim.status,
                "current_round": sim.current_round,
                "twitter_status": sim.twitter_status,
                "reddit_status": sim.reddit_status,
                "error": sim.error,
            }
        }))),
        None => Err(AppError::NotFound(format!("Simulation {} not found", sim_id))),
    }
}

// POST /run/:sim_id/stop
async fn stop_simulation(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);

    match sim_manager.get_simulation(&sim_id).await {
        Some(mut sim) => {
            sim.status = SimulationStatus::Stopped;
            sim_manager.save_state(&sim).await.map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(Json(json!({
                "success": true,
                "data": sim
            })))
        }
        None => Err(AppError::NotFound(format!("Simulation {} not found", sim_id))),
    }
}

// GET /:sim_id
async fn get_simulation(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);

    match sim_manager.get_simulation(&sim_id).await {
        Some(sim) => Ok(Json(json!({
            "success": true,
            "data": sim
        }))),
        None => Err(AppError::NotFound(format!("Simulation {} not found", sim_id))),
    }
}

// GET /list
#[derive(Deserialize)]
struct ListSimulationsQuery {
    project_id: Option<String>,
}

async fn list_simulations(
    State(state): State<AppState>,
    Query(params): Query<ListSimulationsQuery>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let sims = sim_manager.list_simulations(params.project_id.as_deref()).await;
    Ok(Json(json!({
        "success": true,
        "data": {
            "simulations": sims,
            "total": sims.len()
        }
    })))
}

// GET /:sim_id/profiles
#[derive(Deserialize)]
struct ProfilesQuery {
    platform: Option<String>,
}

async fn get_profiles(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
    Query(params): Query<ProfilesQuery>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let platform = params.platform.unwrap_or_else(|| "reddit".to_string());

    match sim_manager.get_profiles(&sim_id, &platform).await {
        Ok(profiles) => Ok(Json(json!({
            "success": true,
            "data": {
                "profiles": profiles,
                "platform": platform,
                "simulation_id": sim_id
            }
        }))),
        Err(e) => Err(AppError::NotFound(format!("Profiles not found for {}: {}", sim_id, e))),
    }
}

// GET /:sim_id/config
async fn get_config(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);

    match sim_manager.get_config(&sim_id).await {
        Some(config) => Ok(Json(json!({
            "success": true,
            "data": {
                "config": config,
                "simulation_id": sim_id
            }
        }))),
        None => Err(AppError::NotFound(format!("Config not found for {}", sim_id))),
    }
}

// POST /start (alias for /run with different body shape)
#[derive(Deserialize)]
struct StartRequest {
    simulation_id: String,
    #[serde(default)]
    max_rounds: Option<usize>,
}

async fn start_simulation(
    State(state): State<AppState>,
    Json(body): Json<StartRequest>,
) -> AppResult<Json<Value>> {
    run_simulation(State(state), Json(RunRequest {
        simulation_id: body.simulation_id,
        max_rounds: body.max_rounds,
    })).await
}

// POST /stop (body-based stop)
#[derive(Deserialize)]
struct StopRequest {
    simulation_id: String,
}

async fn stop_simulation_by_body(
    State(state): State<AppState>,
    Json(body): Json<StopRequest>,
) -> AppResult<Json<Value>> {
    stop_simulation(State(state), Path(body.simulation_id)).await
}

// GET /history
async fn simulation_history(
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let sims = sim_manager.list_simulations(None).await;
    let history: Vec<Value> = sims.iter().map(|s| json!({
        "simulation_id": s.simulation_id,
        "project_id": s.project_id,
        "status": s.status,
        "entities_count": s.entities_count,
        "profiles_count": s.profiles_count,
        "created_at": s.created_at,
        "updated_at": s.updated_at,
    })).collect();
    Ok(Json(json!({
        "success": true,
        "data": history,
        "count": history.len()
    })))
}

// GET /:sim_id/profiles/realtime
async fn get_profiles_realtime(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    get_profiles(State(state), Path(sim_id), Query(ProfilesQuery { platform: Some("reddit".into()) })).await
}

// GET /:sim_id/config/realtime
async fn get_config_realtime(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    get_config(State(state), Path(sim_id)).await
}

// GET /:sim_id/config/download
async fn config_download(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    use axum::http::header;

    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let config_path = sim_manager.sim_dir_path(&sim_id).join("simulation_config.json");

    match tokio::fs::read(&config_path).await {
        Ok(bytes) => {
            let headers = [
                (header::CONTENT_TYPE, "application/json"),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"simulation_config.json\""),
            ];
            Ok((headers, bytes).into_response())
        }
        Err(_) => Err(AppError::NotFound(format!("Config file not found for {}", sim_id))),
    }
}

// GET /script/:script_name/download
async fn download_script(
    Path(script_name): Path<String>,
) -> AppResult<axum::response::Response> {
    use axum::response::IntoResponse;
    use axum::http::header;

    // Scripts directory (relative to backend)
    let scripts_dir = std::path::PathBuf::from("scripts");
    let script_path = scripts_dir.join(&script_name);

    match tokio::fs::read(&script_path).await {
        Ok(bytes) => {
            let headers = [
                (header::CONTENT_TYPE, "text/plain"),
                (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", script_name)),
            ];
            Ok((headers, bytes).into_response())
        }
        Err(_) => Err(AppError::NotFound(format!("Script not found: {}", script_name))),
    }
}

// POST /generate-profiles
#[derive(Deserialize)]
struct GenerateProfilesRequest {
    simulation_id: String,
    #[serde(default)]
    use_llm: Option<bool>,
}

async fn generate_profiles_manual(
    State(state): State<AppState>,
    Json(body): Json<GenerateProfilesRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let simulation = sim_manager.get_simulation(&body.simulation_id).await
        .ok_or_else(|| AppError::NotFound(format!("Simulation {} not found", body.simulation_id)))?;

    let task_id = state.task_manager.create_task("generate_profiles", Some(json!({
        "simulation_id": simulation.simulation_id,
    })));
    let task_id_ret = task_id.clone();

    let config = state.config.clone();
    let task_manager = state.task_manager.clone();
    let sim_id = simulation.simulation_id.clone();
    let graph_id = simulation.graph_id.clone();
    let use_llm = body.use_llm.unwrap_or(true);

    tokio::spawn(async move {
        task_manager.update_task(&task_id, Some(TaskStatus::Processing), Some(10), Some("Reading entities..."), None, None);

        let reader = ZepEntityReader::new(&config.zep_api_key);
        let filtered = match reader.filter_defined_entities(&graph_id, None, true).await {
            Ok(f) => f,
            Err(e) => { task_manager.fail_task(&task_id, &e.to_string()); return; }
        };

        task_manager.update_task(&task_id, None, Some(30), Some("Generating profiles..."), None, None);

        let llm = LLMClient::new(&config.llm_api_key, &config.llm_base_url, &config.llm_model_name);
        let gen = ProfileGenerator::new(llm);
        let profiles = match gen.generate_profiles(&filtered.entities, use_llm).await {
            Ok(p) => p,
            Err(e) => { task_manager.fail_task(&task_id, &e.to_string()); return; }
        };

        let sim_manager = SimulationManager::new(&config.simulation_data_dir);
        let sim_dir = sim_manager.sim_dir_path(&sim_id);
        ProfileGenerator::save_profiles_json(&profiles, &sim_dir.join("reddit_profiles.json")).ok();
        ProfileGenerator::save_profiles_csv(&profiles, &sim_dir.join("twitter_profiles.csv")).ok();

        task_manager.complete_task(&task_id, json!({
            "profiles_count": profiles.len(),
            "simulation_id": sim_id,
        }));
    });

    Ok(Json(json!({
        "success": true,
        "data": {
            "task_id": task_id_ret,
            "simulation_id": body.simulation_id,
            "message": "Profile generation started"
        }
    })))
}

// GET /:sim_id/run-status/detail
async fn run_status_detail(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    match sim_manager.get_simulation(&sim_id).await {
        Some(sim) => {
            let actions = sim_manager.get_actions(&sim_id).await;
            Ok(Json(json!({
                "success": true,
                "data": {
                    "simulation_id": sim.simulation_id,
                    "status": sim.status,
                    "current_round": sim.current_round,
                    "twitter_status": sim.twitter_status,
                    "reddit_status": sim.reddit_status,
                    "total_actions": actions.len(),
                    "entities_count": sim.entities_count,
                    "profiles_count": sim.profiles_count,
                    "error": sim.error,
                }
            })))
        }
        None => Err(AppError::NotFound(format!("Simulation {} not found", sim_id))),
    }
}

// GET /:sim_id/actions
#[derive(Deserialize)]
struct ActionsQuery {
    #[serde(default)]
    round: Option<usize>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn get_actions(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
    Query(params): Query<ActionsQuery>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let mut actions = sim_manager.get_actions(&sim_id).await;

    if let Some(round) = params.round {
        actions.retain(|a| a["round"].as_u64() == Some(round as u64));
    }
    if let Some(ref agent) = params.agent {
        actions.retain(|a| a["agent"].as_str() == Some(agent));
    }
    let total = actions.len();
    if let Some(limit) = params.limit {
        actions.truncate(limit);
    }

    Ok(Json(json!({
        "success": true,
        "data": {
            "actions": actions,
            "total": total,
            "simulation_id": sim_id
        }
    })))
}

// GET /:sim_id/timeline
async fn get_timeline(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let timeline = sim_manager.get_timeline(&sim_id).await;
    Ok(Json(json!({
        "success": true,
        "data": {
            "timeline": timeline,
            "total_rounds": timeline.len(),
            "simulation_id": sim_id
        }
    })))
}

// GET /:sim_id/agent-stats
async fn get_agent_stats(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let stats = sim_manager.get_agent_stats(&sim_id).await;
    Ok(Json(json!({
        "success": true,
        "data": stats
    })))
}

// GET /:sim_id/posts
async fn get_posts(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let posts = sim_manager.get_posts(&sim_id).await;
    Ok(Json(json!({
        "success": true,
        "data": {
            "posts": posts,
            "count": posts.len(),
            "simulation_id": sim_id
        }
    })))
}

// GET /:sim_id/comments
async fn get_comments(
    State(state): State<AppState>,
    Path(sim_id): Path<String>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let comments = sim_manager.get_comments(&sim_id).await;
    Ok(Json(json!({
        "success": true,
        "data": {
            "comments": comments,
            "count": comments.len(),
            "simulation_id": sim_id
        }
    })))
}

// POST /interview - Interview a single agent
#[derive(Deserialize)]
struct InterviewRequest {
    simulation_id: String,
    agent_name: Option<String>,
    agent_id: Option<usize>,
    message: String,
}

async fn interview_agent(
    State(state): State<AppState>,
    Json(body): Json<InterviewRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let _simulation = sim_manager.get_simulation(&body.simulation_id).await
        .ok_or_else(|| AppError::NotFound(format!("Simulation {} not found", body.simulation_id)))?;

    let llm = LLMClient::new(
        &state.config.llm_api_key,
        &state.config.llm_base_url,
        &state.config.llm_model_name,
    );

    // Load profiles to find the agent
    let profiles = sim_manager.get_profiles(&body.simulation_id, "reddit").await
        .unwrap_or(json!([]));
    let profiles_arr = profiles.as_array().cloned().unwrap_or_default();

    let agent_profile = if let Some(ref name) = body.agent_name {
        profiles_arr.iter().find(|p| p["name"].as_str() == Some(name) || p["username"].as_str() == Some(name)).cloned()
    } else if let Some(id) = body.agent_id {
        profiles_arr.iter().find(|p| p["user_id"].as_u64() == Some(id as u64)).cloned()
    } else {
        profiles_arr.first().cloned()
    };

    let persona = agent_profile.as_ref()
        .and_then(|p| p["persona"].as_str())
        .unwrap_or("I am a simulation participant.");
    let agent_name = agent_profile.as_ref()
        .and_then(|p| p["name"].as_str())
        .unwrap_or("Agent");

    let prompt = format!(
        "You are {}. Stay in character.\n\nPersona: {}\n\nRespond to the following without using any tools, just reply as text:\n{}",
        agent_name, persona, body.message
    );

    let messages = vec![
        crate::services::llm_client::ChatMessage { role: "user".into(), content: prompt },
    ];

    match llm.chat(&messages, 0.7, 2048, None).await {
        Ok(response) => Ok(Json(json!({
            "success": true,
            "data": {
                "agent_name": agent_name,
                "response": response,
                "simulation_id": body.simulation_id
            }
        }))),
        Err(e) => Err(AppError::Internal(format!("Interview failed: {}", e))),
    }
}

// POST /interview/batch
#[derive(Deserialize)]
struct InterviewBatchRequest {
    simulation_id: String,
    agent_names: Option<Vec<String>>,
    agent_ids: Option<Vec<usize>>,
    message: String,
}

async fn interview_batch(
    State(state): State<AppState>,
    Json(body): Json<InterviewBatchRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let _ = sim_manager.get_simulation(&body.simulation_id).await
        .ok_or_else(|| AppError::NotFound(format!("Simulation {} not found", body.simulation_id)))?;

    let profiles = sim_manager.get_profiles(&body.simulation_id, "reddit").await
        .unwrap_or(json!([]));
    let profiles_arr = profiles.as_array().cloned().unwrap_or_default();

    let selected: Vec<&Value> = if let Some(ref names) = body.agent_names {
        profiles_arr.iter()
            .filter(|p| {
                let n = p["name"].as_str().unwrap_or("");
                let u = p["username"].as_str().unwrap_or("");
                names.contains(&n.to_string()) || names.contains(&u.to_string())
            })
            .collect()
    } else if let Some(ref ids) = body.agent_ids {
        profiles_arr.iter()
            .filter(|p| {
                p["user_id"].as_u64().map(|id| ids.contains(&(id as usize))).unwrap_or(false)
            })
            .collect()
    } else {
        profiles_arr.iter().take(5).collect()
    };

    let llm = LLMClient::new(
        &state.config.llm_api_key,
        &state.config.llm_base_url,
        &state.config.llm_model_name,
    );

    let mut results = Vec::new();
    for profile in selected {
        let name = profile["name"].as_str().unwrap_or("Agent");
        let persona = profile["persona"].as_str().unwrap_or("");
        let prompt = format!(
            "You are {}. Persona: {}\n\nRespond directly:\n{}",
            name, persona, body.message
        );
        let messages = vec![
            crate::services::llm_client::ChatMessage { role: "user".into(), content: prompt },
        ];
        match llm.chat(&messages, 0.7, 1024, None).await {
            Ok(resp) => results.push(json!({"agent_name": name, "response": resp})),
            Err(e) => results.push(json!({"agent_name": name, "error": e.to_string()})),
        }
    }

    Ok(Json(json!({
        "success": true,
        "data": {
            "responses": results,
            "count": results.len(),
            "simulation_id": body.simulation_id
        }
    })))
}

// POST /interview/all
#[derive(Deserialize)]
struct InterviewAllRequest {
    simulation_id: String,
    message: String,
}

async fn interview_all(
    State(state): State<AppState>,
    Json(body): Json<InterviewAllRequest>,
) -> AppResult<Json<Value>> {
    interview_batch(
        State(state),
        Json(InterviewBatchRequest {
            simulation_id: body.simulation_id,
            agent_names: None,
            agent_ids: None,
            message: body.message,
        }),
    ).await
}

// POST /interview/history
#[derive(Deserialize)]
struct InterviewHistoryRequest {
    simulation_id: String,
    #[serde(default)]
    agent_name: Option<String>,
}

async fn interview_history(
    State(state): State<AppState>,
    Json(body): Json<InterviewHistoryRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    let sim_dir = sim_manager.sim_dir_path(&body.simulation_id);
    let history_path = sim_dir.join("interview_history.json");

    let history: Vec<Value> = if let Ok(data) = tokio::fs::read_to_string(&history_path).await {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        vec![]
    };

    let filtered = if let Some(ref name) = body.agent_name {
        history.into_iter().filter(|h| h["agent_name"].as_str() == Some(name)).collect()
    } else {
        history
    };

    Ok(Json(json!({
        "success": true,
        "data": {
            "history": filtered,
            "count": filtered.len(),
            "simulation_id": body.simulation_id
        }
    })))
}

// POST /env-status
#[derive(Deserialize)]
struct EnvStatusRequest {
    simulation_id: String,
}

async fn env_status(
    State(state): State<AppState>,
    Json(body): Json<EnvStatusRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    match sim_manager.get_simulation(&body.simulation_id).await {
        Some(sim) => Ok(Json(json!({
            "success": true,
            "data": {
                "simulation_id": sim.simulation_id,
                "status": sim.status,
                "env_active": sim.status == SimulationStatus::Running,
                "twitter_active": sim.twitter_status == "running",
                "reddit_active": sim.reddit_status == "running",
            }
        }))),
        None => Err(AppError::NotFound(format!("Simulation {} not found", body.simulation_id))),
    }
}

// POST /close-env
#[derive(Deserialize)]
struct CloseEnvRequest {
    simulation_id: String,
}

async fn close_env(
    State(state): State<AppState>,
    Json(body): Json<CloseEnvRequest>,
) -> AppResult<Json<Value>> {
    let sim_manager = SimulationManager::new(&state.config.simulation_data_dir);
    match sim_manager.get_simulation(&body.simulation_id).await {
        Some(mut sim) => {
            if sim.status == SimulationStatus::Running {
                sim.status = SimulationStatus::Stopped;
                sim.twitter_status = "stopped".to_string();
                sim.reddit_status = "stopped".to_string();
                sim_manager.save_state(&sim).await.map_err(|e| AppError::Internal(e.to_string()))?;
            }
            Ok(Json(json!({
                "success": true,
                "data": {
                    "simulation_id": sim.simulation_id,
                    "status": sim.status,
                    "message": "Environment closed"
                }
            })))
        }
        None => Err(AppError::NotFound(format!("Simulation {} not found", body.simulation_id))),
    }
}
