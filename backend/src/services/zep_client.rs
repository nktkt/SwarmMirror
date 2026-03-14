use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use anyhow::Result;

#[derive(Clone)]
pub struct ZepClient {
    client: Client,
    api_key: String,
    base_url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ZepNode {
    #[serde(alias = "uuid_")]
    pub uuid: String,
    pub name: Option<String>,
    pub labels: Option<Vec<String>>,
    pub summary: Option<String>,
    pub attributes: Option<Value>,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ZepEdge {
    #[serde(alias = "uuid_")]
    pub uuid: String,
    pub name: Option<String>,
    pub fact: Option<String>,
    pub fact_type: Option<String>,
    pub source_node_uuid: String,
    pub target_node_uuid: String,
    pub attributes: Option<Value>,
    pub created_at: Option<String>,
    pub valid_at: Option<String>,
    pub invalid_at: Option<String>,
    pub expired_at: Option<String>,
    pub episodes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct EpisodeResult {
    #[serde(alias = "uuid_")]
    uuid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EpisodeStatus {
    processed: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SearchEdge {
    pub fact: Option<String>,
    pub name: Option<String>,
    pub source_node_uuid: Option<String>,
    pub target_node_uuid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    pub edges: Option<Vec<SearchEdge>>,
}

impl ZepClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            base_url: "https://api.getzep.com/api/v2".to_string(),
        }
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Authorization", format!("Api-Key {}", self.api_key).parse().unwrap());
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers
    }

    pub async fn create_graph(&self, graph_id: &str, name: &str) -> Result<()> {
        let url = format!("{}/graph", self.base_url);
        let resp = self.client.post(&url)
            .headers(self.headers())
            .json(&json!({
                "graph_id": graph_id,
                "name": name,
                "description": "MiroFish Social Simulation Graph"
            }))
            .send().await?;
        if !resp.status().is_success() {
            let body = resp.text().await?;
            anyhow::bail!("Failed to create graph: {}", body);
        }
        Ok(())
    }

    pub async fn set_ontology(&self, graph_id: &str, ontology: &Value) -> Result<()> {
        let url = format!("{}/graph/ontology", self.base_url);

        // Build entity definitions
        let mut entities = json!({});
        if let Some(entity_types) = ontology["entity_types"].as_array() {
            for et in entity_types {
                let name = et["name"].as_str().unwrap_or("Entity");
                let desc = et["description"].as_str().unwrap_or("");
                let mut attrs = json!({});
                if let Some(attributes) = et["attributes"].as_array() {
                    for attr in attributes {
                        let aname = attr["name"].as_str().unwrap_or("attr");
                        let adesc = attr["description"].as_str().unwrap_or("");
                        attrs[aname] = json!({"type": "text", "description": adesc});
                    }
                }
                entities[name] = json!({
                    "description": desc,
                    "attributes": attrs
                });
            }
        }

        // Build edge definitions
        let mut edges = json!({});
        if let Some(edge_types) = ontology["edge_types"].as_array() {
            for et in edge_types {
                let name = et["name"].as_str().unwrap_or("RELATES_TO");
                let desc = et["description"].as_str().unwrap_or("");
                let source_targets = et.get("source_targets").cloned().unwrap_or(json!([]));
                edges[name] = json!({
                    "description": desc,
                    "source_targets": source_targets
                });
            }
        }

        let resp = self.client.put(&url)
            .headers(self.headers())
            .json(&json!({
                "graph_ids": [graph_id],
                "entities": entities,
                "edges": edges
            }))
            .send().await?;

        if !resp.status().is_success() {
            let body = resp.text().await?;
            anyhow::bail!("Failed to set ontology: {}", body);
        }
        Ok(())
    }

    pub async fn add_text_batch(&self, graph_id: &str, chunks: &[String]) -> Result<Vec<String>> {
        let url = format!("{}/graph/{}/episodes/batch", self.base_url, graph_id);
        let episodes: Vec<Value> = chunks.iter()
            .map(|c| json!({"data": c, "type": "text"}))
            .collect();

        let resp = self.client.post(&url)
            .headers(self.headers())
            .json(&json!({"episodes": episodes}))
            .send().await?;

        if !resp.status().is_success() {
            let body = resp.text().await?;
            anyhow::bail!("Failed to add text batch: {}", body);
        }

        let results: Vec<EpisodeResult> = resp.json().await.unwrap_or_default();
        let uuids = results.into_iter()
            .filter_map(|r| r.uuid)
            .collect();
        Ok(uuids)
    }

    pub async fn check_episode_processed(&self, episode_uuid: &str) -> Result<bool> {
        let url = format!("{}/graph/episodes/{}", self.base_url, episode_uuid);
        let resp = self.client.get(&url)
            .headers(self.headers())
            .send().await?;

        if !resp.status().is_success() {
            return Ok(false);
        }

        let status: EpisodeStatus = resp.json().await?;
        Ok(status.processed.unwrap_or(false))
    }

    pub async fn get_nodes(&self, graph_id: &str) -> Result<Vec<ZepNode>> {
        let mut all_nodes = Vec::new();
        let mut page = 1;
        let page_size = 100;

        loop {
            let url = format!("{}/graph/{}/nodes?page={}&page_size={}", self.base_url, graph_id, page, page_size);
            let resp = self.client.get(&url)
                .headers(self.headers())
                .send().await?;

            if !resp.status().is_success() {
                break;
            }

            let body = resp.text().await?;
            let nodes: Vec<ZepNode> = serde_json::from_str(&body).unwrap_or_default();
            let count = nodes.len();
            all_nodes.extend(nodes);

            if count < page_size {
                break;
            }
            page += 1;
        }
        Ok(all_nodes)
    }

    pub async fn get_edges(&self, graph_id: &str) -> Result<Vec<ZepEdge>> {
        let mut all_edges = Vec::new();
        let mut page = 1;
        let page_size = 100;

        loop {
            let url = format!("{}/graph/{}/edges?page={}&page_size={}", self.base_url, graph_id, page, page_size);
            let resp = self.client.get(&url)
                .headers(self.headers())
                .send().await?;

            if !resp.status().is_success() {
                break;
            }

            let body = resp.text().await?;
            let edges: Vec<ZepEdge> = serde_json::from_str(&body).unwrap_or_default();
            let count = edges.len();
            all_edges.extend(edges);

            if count < page_size {
                break;
            }
            page += 1;
        }
        Ok(all_edges)
    }

    pub async fn get_node(&self, node_uuid: &str) -> Result<ZepNode> {
        let url = format!("{}/graph/nodes/{}", self.base_url, node_uuid);
        let resp = self.client.get(&url)
            .headers(self.headers())
            .send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Node not found: {}", node_uuid);
        }
        Ok(resp.json().await?)
    }

    pub async fn get_node_edges(&self, node_uuid: &str) -> Result<Vec<ZepEdge>> {
        let url = format!("{}/graph/nodes/{}/edges", self.base_url, node_uuid);
        let resp = self.client.get(&url)
            .headers(self.headers())
            .send().await?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        Ok(resp.json().await.unwrap_or_default())
    }

    pub async fn search_graph(&self, graph_id: &str, query: &str, limit: usize) -> Result<Vec<String>> {
        let url = format!("{}/graph/{}/search", self.base_url, graph_id);
        let resp = self.client.post(&url)
            .headers(self.headers())
            .json(&json!({"query": query, "limit": limit}))
            .send().await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let search_resp: SearchResponse = resp.json().await.unwrap_or(SearchResponse { edges: None });
        let facts = search_resp.edges.unwrap_or_default()
            .into_iter()
            .filter_map(|e| e.fact)
            .collect();
        Ok(facts)
    }

    pub async fn delete_graph(&self, graph_id: &str) -> Result<()> {
        let url = format!("{}/graph/{}", self.base_url, graph_id);
        self.client.delete(&url)
            .headers(self.headers())
            .send().await?;
        Ok(())
    }
}
