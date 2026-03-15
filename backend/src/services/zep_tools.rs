use crate::services::llm_client::{ChatMessage, LLMClient};
use crate::services::zep_client::{ZepClient, ZepEdge, ZepNode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tracing::{debug, error, info, warn};

// ═══════════════════════════════════════════════════════════════
// Data Structures
// ═══════════════════════════════════════════════════════════════

/// Search result from graph queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub facts: Vec<String>,
    pub edges: Vec<Value>,
    pub nodes: Vec<Value>,
    pub query: String,
    pub total_count: usize,
}

impl SearchResult {
    pub fn to_dict(&self) -> Value {
        json!({
            "facts": self.facts,
            "edges": self.edges,
            "nodes": self.nodes,
            "query": self.query,
            "total_count": self.total_count
        })
    }

    pub fn to_text(&self) -> String {
        let mut parts = vec![
            format!("搜索查询: {}", self.query),
            format!("找到 {} 条相关信息", self.total_count),
        ];
        if !self.facts.is_empty() {
            parts.push("\n### 相关事实:".to_string());
            for (i, fact) in self.facts.iter().enumerate() {
                parts.push(format!("{}. {}", i + 1, fact));
            }
        }
        parts.join("\n")
    }
}

/// Node information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub uuid: String,
    pub name: String,
    pub labels: Vec<String>,
    pub summary: String,
    pub attributes: Value,
}

impl NodeInfo {
    pub fn empty() -> Self {
        Self {
            uuid: String::new(),
            name: String::new(),
            labels: Vec::new(),
            summary: String::new(),
            attributes: json!({}),
        }
    }

    pub fn to_dict(&self) -> Value {
        json!({
            "uuid": self.uuid,
            "name": self.name,
            "labels": self.labels,
            "summary": self.summary,
            "attributes": self.attributes
        })
    }

    pub fn to_text(&self) -> String {
        let entity_type = self
            .labels
            .iter()
            .find(|l| *l != "Entity" && *l != "Node")
            .cloned()
            .unwrap_or_else(|| "未知类型".to_string());
        format!(
            "实体: {} (类型: {})\n摘要: {}",
            self.name, entity_type, self.summary
        )
    }

    fn from_zep_node(node: &ZepNode) -> Self {
        Self {
            uuid: node.uuid.clone(),
            name: node.name.clone().unwrap_or_default(),
            labels: node.labels.clone().unwrap_or_default(),
            summary: node.summary.clone().unwrap_or_default(),
            attributes: node.attributes.clone().unwrap_or(json!({})),
        }
    }
}

/// Edge information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeInfo {
    pub uuid: String,
    pub name: String,
    pub fact: String,
    pub source_node_uuid: String,
    pub target_node_uuid: String,
    pub source_node_name: Option<String>,
    pub target_node_name: Option<String>,
    pub created_at: Option<String>,
    pub valid_at: Option<String>,
    pub invalid_at: Option<String>,
    pub expired_at: Option<String>,
}

impl EdgeInfo {
    pub fn to_dict(&self) -> Value {
        json!({
            "uuid": self.uuid,
            "name": self.name,
            "fact": self.fact,
            "source_node_uuid": self.source_node_uuid,
            "target_node_uuid": self.target_node_uuid,
            "source_node_name": self.source_node_name,
            "target_node_name": self.target_node_name,
            "created_at": self.created_at,
            "valid_at": self.valid_at,
            "invalid_at": self.invalid_at,
            "expired_at": self.expired_at
        })
    }

    pub fn to_text(&self, include_temporal: bool) -> String {
        let source = self
            .source_node_name
            .as_deref()
            .unwrap_or_else(|| &self.source_node_uuid[..8.min(self.source_node_uuid.len())]);
        let target = self
            .target_node_name
            .as_deref()
            .unwrap_or_else(|| &self.target_node_uuid[..8.min(self.target_node_uuid.len())]);
        let mut base = format!(
            "关系: {} --[{}]--> {}\n事实: {}",
            source, self.name, target, self.fact
        );
        if include_temporal {
            let valid_at = self.valid_at.as_deref().unwrap_or("未知");
            let invalid_at = self.invalid_at.as_deref().unwrap_or("至今");
            base += &format!("\n时效: {} - {}", valid_at, invalid_at);
            if let Some(ref expired) = self.expired_at {
                base += &format!(" (已过期: {})", expired);
            }
        }
        base
    }

    pub fn is_expired(&self) -> bool {
        self.expired_at.is_some()
    }

    pub fn is_invalid(&self) -> bool {
        self.invalid_at.is_some()
    }

    fn from_zep_edge(edge: &ZepEdge, include_temporal: bool) -> Self {
        let mut info = Self {
            uuid: edge.uuid.clone(),
            name: edge.name.clone().unwrap_or_default(),
            fact: edge.fact.clone().unwrap_or_default(),
            source_node_uuid: edge.source_node_uuid.clone(),
            target_node_uuid: edge.target_node_uuid.clone(),
            source_node_name: None,
            target_node_name: None,
            created_at: None,
            valid_at: None,
            invalid_at: None,
            expired_at: None,
        };
        if include_temporal {
            info.created_at = edge.created_at.clone();
            info.valid_at = edge.valid_at.clone();
            info.invalid_at = edge.invalid_at.clone();
            info.expired_at = edge.expired_at.clone();
        }
        info
    }
}

/// Deep insight search result (InsightForge).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightForgeResult {
    pub query: String,
    pub simulation_requirement: String,
    pub sub_queries: Vec<String>,
    pub semantic_facts: Vec<String>,
    pub entity_insights: Vec<Value>,
    pub relationship_chains: Vec<String>,
    pub total_facts: usize,
    pub total_entities: usize,
    pub total_relationships: usize,
}

impl InsightForgeResult {
    pub fn to_dict(&self) -> Value {
        json!({
            "query": self.query,
            "simulation_requirement": self.simulation_requirement,
            "sub_queries": self.sub_queries,
            "semantic_facts": self.semantic_facts,
            "entity_insights": self.entity_insights,
            "relationship_chains": self.relationship_chains,
            "total_facts": self.total_facts,
            "total_entities": self.total_entities,
            "total_relationships": self.total_relationships
        })
    }

    pub fn to_text(&self) -> String {
        let mut parts = vec![
            "## 未来预测深度分析".to_string(),
            format!("分析问题: {}", self.query),
            format!("预测场景: {}", self.simulation_requirement),
            "\n### 预测数据统计".to_string(),
            format!("- 相关预测事实: {}条", self.total_facts),
            format!("- 涉及实体: {}个", self.total_entities),
            format!("- 关系链: {}条", self.total_relationships),
        ];
        if !self.sub_queries.is_empty() {
            parts.push("\n### 分析的子问题".to_string());
            for (i, sq) in self.sub_queries.iter().enumerate() {
                parts.push(format!("{}. {}", i + 1, sq));
            }
        }
        if !self.semantic_facts.is_empty() {
            parts.push("\n### 【关键事实】(请在报告中引用这些原文)".to_string());
            for (i, fact) in self.semantic_facts.iter().enumerate() {
                parts.push(format!("{}. \"{}\"", i + 1, fact));
            }
        }
        if !self.entity_insights.is_empty() {
            parts.push("\n### 【核心实体】".to_string());
            for entity in &self.entity_insights {
                let name = entity["name"].as_str().unwrap_or("未知");
                let etype = entity["type"].as_str().unwrap_or("实体");
                parts.push(format!("- **{}** ({})", name, etype));
                if let Some(summary) = entity["summary"].as_str() {
                    if !summary.is_empty() {
                        parts.push(format!("  摘要: \"{}\"", summary));
                    }
                }
                if let Some(rf) = entity["related_facts"].as_array() {
                    parts.push(format!("  相关事实: {}条", rf.len()));
                }
            }
        }
        if !self.relationship_chains.is_empty() {
            parts.push("\n### 【关系链】".to_string());
            for chain in &self.relationship_chains {
                parts.push(format!("- {}", chain));
            }
        }
        parts.join("\n")
    }
}

/// Panorama (broad overview) search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanoramaResult {
    pub query: String,
    pub all_nodes: Vec<NodeInfo>,
    pub all_edges: Vec<EdgeInfo>,
    pub active_facts: Vec<String>,
    pub historical_facts: Vec<String>,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub active_count: usize,
    pub historical_count: usize,
}

impl PanoramaResult {
    pub fn to_dict(&self) -> Value {
        json!({
            "query": self.query,
            "all_nodes": self.all_nodes.iter().map(|n| n.to_dict()).collect::<Vec<_>>(),
            "all_edges": self.all_edges.iter().map(|e| e.to_dict()).collect::<Vec<_>>(),
            "active_facts": self.active_facts,
            "historical_facts": self.historical_facts,
            "total_nodes": self.total_nodes,
            "total_edges": self.total_edges,
            "active_count": self.active_count,
            "historical_count": self.historical_count
        })
    }

    pub fn to_text(&self) -> String {
        let mut parts = vec![
            "## 广度搜索结果（未来全景视图）".to_string(),
            format!("查询: {}", self.query),
            "\n### 统计信息".to_string(),
            format!("- 总节点数: {}", self.total_nodes),
            format!("- 总边数: {}", self.total_edges),
            format!("- 当前有效事实: {}条", self.active_count),
            format!("- 历史/过期事实: {}条", self.historical_count),
        ];
        if !self.active_facts.is_empty() {
            parts.push("\n### 【当前有效事实】(模拟结果原文)".to_string());
            for (i, fact) in self.active_facts.iter().enumerate() {
                parts.push(format!("{}. \"{}\"", i + 1, fact));
            }
        }
        if !self.historical_facts.is_empty() {
            parts.push("\n### 【历史/过期事实】(演变过程记录)".to_string());
            for (i, fact) in self.historical_facts.iter().enumerate() {
                parts.push(format!("{}. \"{}\"", i + 1, fact));
            }
        }
        if !self.all_nodes.is_empty() {
            parts.push("\n### 【涉及实体】".to_string());
            for node in &self.all_nodes {
                let entity_type = node
                    .labels
                    .iter()
                    .find(|l| *l != "Entity" && *l != "Node")
                    .cloned()
                    .unwrap_or_else(|| "实体".to_string());
                parts.push(format!("- **{}** ({})", node.name, entity_type));
            }
        }
        parts.join("\n")
    }
}

// ═══════════════════════════════════════════════════════════════
// ZepToolsService
// ═══════════════════════════════════════════════════════════════

/// Zep graph retrieval service providing search, insight, and panorama tools.
pub struct ZepToolsService {
    client: ZepClient,
    llm: Option<LLMClient>,
}

/// Retry configuration constants.
const MAX_RETRIES: usize = 3;
const RETRY_DELAY_MS: u64 = 2000;

impl ZepToolsService {
    pub fn new(api_key: &str) -> Self {
        info!("ZepToolsService initialized");
        Self {
            client: ZepClient::new(api_key),
            llm: None,
        }
    }

    pub fn with_llm(api_key: &str, llm: LLMClient) -> Self {
        info!("ZepToolsService initialized with LLM client");
        Self {
            client: ZepClient::new(api_key),
            llm: Some(llm),
        }
    }

    /// Build from an existing ZepClient and LLMClient.
    pub fn from_client(client: ZepClient, llm: LLMClient) -> Self {
        info!("ZepToolsService initialized from existing client");
        Self {
            client,
            llm: Some(llm),
        }
    }

    pub fn set_llm(&mut self, llm: LLMClient) {
        self.llm = Some(llm);
    }

    fn llm(&self) -> Option<&LLMClient> {
        self.llm.as_ref()
    }

    // ─── retry helper ───

    async fn call_with_retry<F, Fut, T>(
        &self,
        operation_name: &str,
        mut f: F,
    ) -> anyhow::Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<T>>,
    {
        let mut last_err: Option<anyhow::Error> = None;
        let mut delay = RETRY_DELAY_MS;
        for attempt in 0..MAX_RETRIES {
            match f().await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if attempt < MAX_RETRIES - 1 {
                        warn!(
                            "Zep {} attempt {} failed: {}. Retrying in {}ms...",
                            operation_name,
                            attempt + 1,
                            e,
                            delay
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        delay *= 2;
                    } else {
                        error!(
                            "Zep {} failed after {} attempts: {}",
                            operation_name, MAX_RETRIES, e
                        );
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap())
    }

    // ═══════════════════════════════════════════════════════════════
    // Basic graph operations
    // ═══════════════════════════════════════════════════════════════

    /// Semantic + BM25 hybrid graph search via Zep Cloud. Falls back to local keyword search.
    pub async fn search_graph(
        &self,
        graph_id: &str,
        query: &str,
        limit: usize,
        _scope: &str,
    ) -> SearchResult {
        let q = if query.len() > 50 {
            &query[..50]
        } else {
            query
        };
        info!(graph_id, query = q, "Graph search");

        let gid = graph_id.to_string();
        let qr = query.to_string();
        match self
            .call_with_retry(&format!("graph_search({})", graph_id), || {
                let c = self.client.clone();
                let g = gid.clone();
                let q = qr.clone();
                async move { c.search_graph(&g, &q, limit).await }
            })
            .await
        {
            Ok(facts) => {
                info!("Search complete: found {} facts", facts.len());
                SearchResult {
                    total_count: facts.len(),
                    query: query.to_string(),
                    facts,
                    edges: Vec::new(),
                    nodes: Vec::new(),
                }
            }
            Err(e) => {
                warn!("Zep Search API failed, falling back to local: {}", e);
                self.local_search(graph_id, query, limit).await
            }
        }
    }

    /// Local keyword-match fallback search.
    async fn local_search(&self, graph_id: &str, query: &str, limit: usize) -> SearchResult {
        info!("Using local search for: {}...", &query[..query.len().min(30)]);
        let query_lower = query.to_lowercase();
        let keywords: Vec<String> = query_lower
            .replace(',', " ")
            .replace('，', " ")
            .split_whitespace()
            .filter(|w| w.len() > 1)
            .map(|s| s.to_string())
            .collect();

        let match_score = |text: &str| -> i32 {
            if text.is_empty() {
                return 0;
            }
            let text_lower = text.to_lowercase();
            if text_lower.contains(&query_lower) {
                return 100;
            }
            let mut score = 0i32;
            for kw in &keywords {
                if text_lower.contains(kw.as_str()) {
                    score += 10;
                }
            }
            score
        };

        let mut facts = Vec::new();
        let mut edges_result = Vec::new();

        if let Ok(all_edges) = self.get_all_edges(graph_id, false).await {
            let mut scored: Vec<(i32, &EdgeInfo)> = all_edges
                .iter()
                .map(|e| (match_score(&e.fact) + match_score(&e.name), e))
                .filter(|(s, _)| *s > 0)
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            for (_, edge) in scored.iter().take(limit) {
                if !edge.fact.is_empty() {
                    facts.push(edge.fact.clone());
                }
                edges_result.push(json!({
                    "uuid": edge.uuid,
                    "name": edge.name,
                    "fact": edge.fact,
                    "source_node_uuid": edge.source_node_uuid,
                    "target_node_uuid": edge.target_node_uuid,
                }));
            }
        }

        info!("Local search complete: found {} facts", facts.len());
        SearchResult {
            total_count: facts.len(),
            query: query.to_string(),
            facts,
            edges: edges_result,
            nodes: Vec::new(),
        }
    }

    /// Get all nodes in a graph (paginated fetch).
    pub async fn get_all_nodes(&self, graph_id: &str) -> anyhow::Result<Vec<NodeInfo>> {
        info!(graph_id, "Fetching all nodes");
        let nodes = self.client.get_nodes(graph_id).await?;
        let result: Vec<NodeInfo> = nodes.iter().map(NodeInfo::from_zep_node).collect();
        info!("Fetched {} nodes", result.len());
        Ok(result)
    }

    /// Get all edges in a graph (paginated fetch, with temporal info).
    pub async fn get_all_edges(
        &self,
        graph_id: &str,
        include_temporal: bool,
    ) -> anyhow::Result<Vec<EdgeInfo>> {
        info!(graph_id, "Fetching all edges");
        let edges = self.client.get_edges(graph_id).await?;
        let result: Vec<EdgeInfo> = edges
            .iter()
            .map(|e| EdgeInfo::from_zep_edge(e, include_temporal))
            .collect();
        info!("Fetched {} edges", result.len());
        Ok(result)
    }

    /// Get detailed info for a single node.
    pub async fn get_node_detail(&self, node_uuid: &str) -> Option<NodeInfo> {
        debug!(node_uuid, "Getting node detail");
        match self.client.get_node(node_uuid).await {
            Ok(node) => Some(NodeInfo::from_zep_node(&node)),
            Err(e) => {
                error!("Failed to get node detail: {}", e);
                None
            }
        }
    }

    /// Get all edges connected to a specific node.
    pub async fn get_node_edges(&self, graph_id: &str, node_uuid: &str) -> Vec<EdgeInfo> {
        debug!(node_uuid, "Getting node edges");
        match self.get_all_edges(graph_id, true).await {
            Ok(all_edges) => {
                let result: Vec<EdgeInfo> = all_edges
                    .into_iter()
                    .filter(|e| e.source_node_uuid == node_uuid || e.target_node_uuid == node_uuid)
                    .collect();
                info!("Found {} edges for node", result.len());
                result
            }
            Err(e) => {
                warn!("Failed to get node edges: {}", e);
                Vec::new()
            }
        }
    }

    /// Get entities filtered by type label.
    pub async fn get_entities_by_type(
        &self,
        graph_id: &str,
        entity_type: &str,
    ) -> Vec<NodeInfo> {
        info!(entity_type, "Getting entities by type");
        match self.get_all_nodes(graph_id).await {
            Ok(nodes) => {
                let filtered: Vec<NodeInfo> = nodes
                    .into_iter()
                    .filter(|n| n.labels.iter().any(|l| l == entity_type))
                    .collect();
                info!("Found {} entities of type {}", filtered.len(), entity_type);
                filtered
            }
            Err(_) => Vec::new(),
        }
    }

    /// Get entity relationship summary by searching for name and collecting edges.
    pub async fn get_entity_summary(
        &self,
        graph_id: &str,
        entity_name: &str,
    ) -> Value {
        info!(entity_name, "Getting entity summary");
        let search_result = self.search_graph(graph_id, entity_name, 20, "edges").await;

        // Find the entity node
        let all_nodes = self.get_all_nodes(graph_id).await.unwrap_or_default();
        let entity_node = all_nodes
            .iter()
            .find(|n| n.name.to_lowercase() == entity_name.to_lowercase());

        let related_edges = if let Some(node) = entity_node {
            self.get_node_edges(graph_id, &node.uuid).await
        } else {
            Vec::new()
        };

        json!({
            "entity_name": entity_name,
            "entity_info": entity_node.map(|n| n.to_dict()),
            "related_facts": search_result.facts,
            "related_edges": related_edges.iter().map(|e| e.to_dict()).collect::<Vec<_>>(),
            "total_relations": related_edges.len()
        })
    }

    /// Get statistics about the graph: node/edge counts, type distributions.
    pub async fn get_graph_statistics(&self, graph_id: &str) -> Value {
        info!(graph_id, "Getting graph statistics");
        let nodes = self.get_all_nodes(graph_id).await.unwrap_or_default();
        let edges = self.get_all_edges(graph_id, false).await.unwrap_or_default();

        let mut entity_types: HashMap<String, usize> = HashMap::new();
        for node in &nodes {
            for label in &node.labels {
                if label != "Entity" && label != "Node" {
                    *entity_types.entry(label.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut relation_types: HashMap<String, usize> = HashMap::new();
        for edge in &edges {
            *relation_types.entry(edge.name.clone()).or_insert(0) += 1;
        }

        json!({
            "graph_id": graph_id,
            "total_nodes": nodes.len(),
            "total_edges": edges.len(),
            "entity_types": entity_types,
            "relation_types": relation_types
        })
    }

    /// Get comprehensive simulation context: related facts, statistics, entities.
    pub async fn get_simulation_context(
        &self,
        graph_id: &str,
        simulation_requirement: &str,
        limit: usize,
    ) -> Value {
        info!("Getting simulation context");
        let search_result = self
            .search_graph(graph_id, simulation_requirement, limit, "edges")
            .await;
        let stats = self.get_graph_statistics(graph_id).await;
        let all_nodes = self.get_all_nodes(graph_id).await.unwrap_or_default();

        let mut entities: Vec<Value> = Vec::new();
        for node in &all_nodes {
            let custom_labels: Vec<&String> = node
                .labels
                .iter()
                .filter(|l| *l != "Entity" && *l != "Node")
                .collect();
            if !custom_labels.is_empty() {
                entities.push(json!({
                    "name": node.name,
                    "type": custom_labels[0],
                    "summary": node.summary
                }));
            }
        }
        let total_entities = entities.len();
        entities.truncate(limit);

        json!({
            "simulation_requirement": simulation_requirement,
            "related_facts": search_result.facts,
            "graph_statistics": stats,
            "entities": entities,
            "total_entities": total_entities
        })
    }

    // ═══════════════════════════════════════════════════════════════
    // Core retrieval tools (optimized)
    // ═══════════════════════════════════════════════════════════════

    /// InsightForge - Deep insight retrieval.
    ///
    /// 1. Use LLM to generate sub-queries from the main query
    /// 2. Search graph with each sub-query
    /// 3. Extract related entities and get their details
    /// 4. Track relationship chains
    /// 5. Return combined insights
    pub async fn insight_forge(
        &self,
        graph_id: &str,
        query: &str,
        simulation_requirement: &str,
        report_context: &str,
        max_sub_queries: usize,
    ) -> InsightForgeResult {
        info!("InsightForge deep search: {}...", &query[..query.len().min(50)]);

        let mut result = InsightForgeResult {
            query: query.to_string(),
            simulation_requirement: simulation_requirement.to_string(),
            sub_queries: Vec::new(),
            semantic_facts: Vec::new(),
            entity_insights: Vec::new(),
            relationship_chains: Vec::new(),
            total_facts: 0,
            total_entities: 0,
            total_relationships: 0,
        };

        // Step 1: Generate sub-queries via LLM
        let sub_queries = self
            .generate_sub_queries(query, simulation_requirement, report_context, max_sub_queries)
            .await;
        result.sub_queries = sub_queries.clone();
        info!("Generated {} sub-queries", sub_queries.len());

        // Step 2: Search with each sub-query
        let mut all_facts = Vec::new();
        let mut all_edges: Vec<Value> = Vec::new();
        let mut seen_facts: HashSet<String> = HashSet::new();

        for sq in &sub_queries {
            let sr = self.search_graph(graph_id, sq, 15, "edges").await;
            for fact in &sr.facts {
                if seen_facts.insert(fact.clone()) {
                    all_facts.push(fact.clone());
                }
            }
            all_edges.extend(sr.edges);
        }

        // Also search the main query
        let main_sr = self.search_graph(graph_id, query, 20, "edges").await;
        for fact in &main_sr.facts {
            if seen_facts.insert(fact.clone()) {
                all_facts.push(fact.clone());
            }
        }

        result.semantic_facts = all_facts.clone();
        result.total_facts = all_facts.len();

        // Step 3: Extract entity UUIDs from edges and get details
        let mut entity_uuids: HashSet<String> = HashSet::new();
        for edge_data in &all_edges {
            if let Some(src) = edge_data["source_node_uuid"].as_str() {
                if !src.is_empty() {
                    entity_uuids.insert(src.to_string());
                }
            }
            if let Some(tgt) = edge_data["target_node_uuid"].as_str() {
                if !tgt.is_empty() {
                    entity_uuids.insert(tgt.to_string());
                }
            }
        }

        let mut entity_insights = Vec::new();
        let mut node_map: HashMap<String, NodeInfo> = HashMap::new();

        for uuid in &entity_uuids {
            if uuid.is_empty() {
                continue;
            }
            if let Some(node) = self.get_node_detail(uuid).await {
                node_map.insert(uuid.clone(), node.clone());
                let entity_type = node
                    .labels
                    .iter()
                    .find(|l| *l != "Entity" && *l != "Node")
                    .cloned()
                    .unwrap_or_else(|| "实体".to_string());

                let name_lower = node.name.to_lowercase();
                let related_facts: Vec<&String> = all_facts
                    .iter()
                    .filter(|f| f.to_lowercase().contains(&name_lower))
                    .collect();

                entity_insights.push(json!({
                    "uuid": node.uuid,
                    "name": node.name,
                    "type": entity_type,
                    "summary": node.summary,
                    "related_facts": related_facts
                }));
            }
        }

        result.entity_insights = entity_insights;
        result.total_entities = result.entity_insights.len();

        // Step 4: Build relationship chains
        let mut relationship_chains = Vec::new();
        for edge_data in &all_edges {
            let source_uuid = edge_data["source_node_uuid"].as_str().unwrap_or("");
            let target_uuid = edge_data["target_node_uuid"].as_str().unwrap_or("");
            let rel_name = edge_data["name"].as_str().unwrap_or("");

            let source_name = node_map
                .get(source_uuid)
                .map(|n| n.name.as_str())
                .unwrap_or_else(|| &source_uuid[..source_uuid.len().min(8)]);
            let target_name = node_map
                .get(target_uuid)
                .map(|n| n.name.as_str())
                .unwrap_or_else(|| &target_uuid[..target_uuid.len().min(8)]);

            let chain = format!("{} --[{}]--> {}", source_name, rel_name, target_name);
            if !relationship_chains.contains(&chain) {
                relationship_chains.push(chain);
            }
        }

        result.relationship_chains = relationship_chains;
        result.total_relationships = result.relationship_chains.len();

        info!(
            "InsightForge complete: {} facts, {} entities, {} relationships",
            result.total_facts, result.total_entities, result.total_relationships
        );
        result
    }

    /// Generate sub-queries from a main query using LLM.
    async fn generate_sub_queries(
        &self,
        query: &str,
        simulation_requirement: &str,
        report_context: &str,
        max_queries: usize,
    ) -> Vec<String> {
        let llm = match self.llm() {
            Some(l) => l,
            None => {
                // Fallback: return simple variants of the original query
                return vec![
                    query.to_string(),
                    format!("{} 的主要参与者", query),
                    format!("{} 的原因和影响", query),
                    format!("{} 的发展过程", query),
                ]
                .into_iter()
                .take(max_queries)
                .collect();
            }
        };

        let system_prompt = "你是一个专业的问题分析专家。你的任务是将一个复杂问题分解为多个可以在模拟世界中独立观察的子问题。\n\n\
            要求：\n\
            1. 每个子问题应该足够具体，可以在模拟世界中找到相关的Agent行为或事件\n\
            2. 子问题应该覆盖原问题的不同维度（如：谁、什么、为什么、怎么样、何时、何地）\n\
            3. 子问题应该与模拟场景相关\n\
            4. 返回JSON格式：{\"sub_queries\": [\"子问题1\", \"子问题2\", ...]}";

        let ctx_part = if report_context.is_empty() {
            String::new()
        } else {
            let truncated = &report_context[..report_context.len().min(500)];
            format!("\n报告上下文：{}", truncated)
        };

        let user_prompt = format!(
            "模拟需求背景：\n{}\n{}\n\n请将以下问题分解为{}个子问题：\n{}\n\n返回JSON格式的子问题列表。",
            simulation_requirement, ctx_part, max_queries, query
        );

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".into(),
                content: user_prompt,
            },
        ];

        match llm.chat_json(&messages, 0.3, 2048).await {
            Ok(resp) => {
                if let Some(sqs) = resp["sub_queries"].as_array() {
                    sqs.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .take(max_queries)
                        .collect()
                } else {
                    vec![query.to_string()]
                }
            }
            Err(e) => {
                warn!("Failed to generate sub-queries: {}", e);
                vec![
                    query.to_string(),
                    format!("{} 的主要参与者", query),
                    format!("{} 的原因和影响", query),
                    format!("{} 的发展过程", query),
                ]
                .into_iter()
                .take(max_queries)
                .collect()
            }
        }
    }

    /// PanoramaSearch - Broad overview search.
    ///
    /// Gets all nodes and edges, classifies facts into active/historical,
    /// and sorts by relevance.
    pub async fn panorama_search(
        &self,
        graph_id: &str,
        query: &str,
        include_expired: bool,
        limit: usize,
    ) -> PanoramaResult {
        info!("PanoramaSearch: {}...", &query[..query.len().min(50)]);

        let all_nodes = self.get_all_nodes(graph_id).await.unwrap_or_default();
        let _node_map: HashMap<String, &NodeInfo> =
            all_nodes.iter().map(|n| (n.uuid.clone(), n)).collect();

        let all_edges = self.get_all_edges(graph_id, true).await.unwrap_or_default();

        let mut active_facts = Vec::new();
        let mut historical_facts = Vec::new();

        for edge in &all_edges {
            if edge.fact.is_empty() {
                continue;
            }
            let is_historical = edge.is_expired() || edge.is_invalid();
            if is_historical {
                let valid_at = edge.valid_at.as_deref().unwrap_or("未知");
                let invalid_at = edge
                    .invalid_at
                    .as_deref()
                    .or(edge.expired_at.as_deref())
                    .unwrap_or("未知");
                historical_facts.push(format!("[{} - {}] {}", valid_at, invalid_at, edge.fact));
            } else {
                active_facts.push(edge.fact.clone());
            }
        }

        // Sort by relevance to query
        let query_lower = query.to_lowercase();
        let keywords: Vec<String> = query_lower
            .replace(',', " ")
            .replace('，', " ")
            .split_whitespace()
            .filter(|w| w.len() > 1)
            .map(|s| s.to_string())
            .collect();

        let relevance_score = |fact: &str| -> i32 {
            let fl = fact.to_lowercase();
            let mut score = 0i32;
            if fl.contains(&query_lower) {
                score += 100;
            }
            for kw in &keywords {
                if fl.contains(kw.as_str()) {
                    score += 10;
                }
            }
            score
        };

        active_facts.sort_by(|a, b| relevance_score(b).cmp(&relevance_score(a)));
        historical_facts.sort_by(|a, b| relevance_score(b).cmp(&relevance_score(a)));

        let active_count = active_facts.len();
        let historical_count = historical_facts.len();

        active_facts.truncate(limit);
        let historical_limited = if include_expired {
            historical_facts.into_iter().take(limit).collect()
        } else {
            Vec::new()
        };

        let total_nodes = all_nodes.len();
        let total_edges = all_edges.len();

        info!(
            "PanoramaSearch complete: {} active, {} historical",
            active_count, historical_count
        );

        PanoramaResult {
            query: query.to_string(),
            all_nodes,
            all_edges,
            active_facts,
            historical_facts: historical_limited,
            total_nodes,
            total_edges,
            active_count,
            historical_count,
        }
    }

    /// QuickSearch - Simple, lightweight search.
    pub async fn quick_search(
        &self,
        graph_id: &str,
        query: &str,
        limit: usize,
    ) -> SearchResult {
        info!("QuickSearch: {}...", &query[..query.len().min(50)]);
        let result = self.search_graph(graph_id, query, limit, "edges").await;
        info!("QuickSearch complete: {} results", result.total_count);
        result
    }

    /// Format search results for LLM consumption.
    pub fn format_facts_for_llm(facts: &[String]) -> String {
        if facts.is_empty() {
            return "No specific facts found.".to_string();
        }
        facts
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
