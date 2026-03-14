use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Config {
    pub secret_key: String,
    pub debug: bool,
    pub llm_api_key: String,
    pub llm_base_url: String,
    pub llm_model_name: String,
    pub llm_boost_api_key: Option<String>,
    pub llm_boost_base_url: Option<String>,
    pub llm_boost_model_name: Option<String>,
    pub zep_api_key: String,
    pub max_content_length: usize,
    pub upload_folder: PathBuf,
    pub allowed_extensions: Vec<String>,
    pub default_chunk_size: usize,
    pub default_chunk_overlap: usize,
    pub oasis_default_max_rounds: usize,
    pub simulation_data_dir: PathBuf,
    pub oasis_twitter_actions: Vec<String>,
    pub oasis_reddit_actions: Vec<String>,
    pub report_agent_max_tool_calls: usize,
    pub report_agent_max_reflection_rounds: usize,
    pub report_agent_temperature: f64,
}

impl Config {
    pub fn from_env() -> Self {
        let upload_folder = PathBuf::from(
            env::var("UPLOAD_FOLDER").unwrap_or_else(|_| "./uploads".to_string())
        );
        let simulation_data_dir = upload_folder.join("simulations");

        Self {
            secret_key: env::var("SECRET_KEY").unwrap_or_else(|_| "mirofish-secret-key".into()),
            debug: env::var("FLASK_DEBUG").unwrap_or_else(|_| "true".into()).to_lowercase() == "true",
            llm_api_key: env::var("LLM_API_KEY").unwrap_or_default(),
            llm_base_url: env::var("LLM_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            llm_model_name: env::var("LLM_MODEL_NAME").unwrap_or_else(|_| "gpt-4o-mini".into()),
            llm_boost_api_key: env::var("LLM_BOOST_API_KEY").ok(),
            llm_boost_base_url: env::var("LLM_BOOST_BASE_URL").ok(),
            llm_boost_model_name: env::var("LLM_BOOST_MODEL_NAME").ok(),
            zep_api_key: env::var("ZEP_API_KEY").unwrap_or_default(),
            max_content_length: 50 * 1024 * 1024,
            upload_folder,
            allowed_extensions: vec!["pdf".into(), "md".into(), "txt".into(), "markdown".into()],
            default_chunk_size: 500,
            default_chunk_overlap: 50,
            oasis_default_max_rounds: env::var("OASIS_DEFAULT_MAX_ROUNDS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(10),
            simulation_data_dir,
            oasis_twitter_actions: vec![
                "CREATE_POST".into(), "LIKE_POST".into(), "REPOST".into(),
                "FOLLOW".into(), "DO_NOTHING".into(), "QUOTE_POST".into(),
            ],
            oasis_reddit_actions: vec![
                "LIKE_POST".into(), "DISLIKE_POST".into(), "CREATE_POST".into(),
                "CREATE_COMMENT".into(), "LIKE_COMMENT".into(), "DISLIKE_COMMENT".into(),
                "SEARCH_POSTS".into(), "SEARCH_USER".into(), "TREND".into(),
                "REFRESH".into(), "DO_NOTHING".into(), "FOLLOW".into(), "MUTE".into(),
            ],
            report_agent_max_tool_calls: env::var("REPORT_AGENT_MAX_TOOL_CALLS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(5),
            report_agent_max_reflection_rounds: env::var("REPORT_AGENT_MAX_REFLECTION_ROUNDS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(2),
            report_agent_temperature: env::var("REPORT_AGENT_TEMPERATURE")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(0.5),
        }
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.llm_api_key.is_empty() {
            errors.push("LLM_API_KEY not configured".into());
        }
        if self.zep_api_key.is_empty() {
            errors.push("ZEP_API_KEY not configured".into());
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn is_allowed_extension(&self, filename: &str) -> bool {
        if let Some(ext) = std::path::Path::new(filename).extension() {
            self.allowed_extensions.contains(&ext.to_string_lossy().to_lowercase())
        } else {
            false
        }
    }
}
