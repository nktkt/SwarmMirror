use crate::services::zep_client::ZepClient;
use serde_json::{json, Value};

pub struct ZepToolsService {
    client: ZepClient,
}

impl ZepToolsService {
    pub fn new(api_key: &str) -> Self {
        Self { client: ZepClient::new(api_key) }
    }

    pub async fn search_graph(&self, graph_id: &str, query: &str, limit: usize) -> anyhow::Result<Value> {
        let facts = self.client.search_graph(graph_id, query, limit).await?;

        Ok(json!({
            "facts": facts,
            "query": query,
            "total_count": facts.len(),
        }))
    }

    pub async fn get_statistics(&self, graph_id: &str) -> anyhow::Result<Value> {
        let nodes = self.client.get_nodes(graph_id).await?;
        let edges = self.client.get_edges(graph_id).await?;

        let mut type_counts = std::collections::HashMap::new();
        for node in &nodes {
            for label in node.labels.as_ref().unwrap_or(&vec![]) {
                if label != "Entity" && label != "Node" {
                    *type_counts.entry(label.clone()).or_insert(0usize) += 1;
                }
            }
        }

        Ok(json!({
            "graph_id": graph_id,
            "node_count": nodes.len(),
            "edge_count": edges.len(),
            "entity_type_counts": type_counts,
        }))
    }
}
