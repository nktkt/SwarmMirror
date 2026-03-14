use crate::services::zep_client::ZepClient;
use serde_json::Value;
use uuid::Uuid;

pub struct GraphBuilderService {
    client: ZepClient,
}

impl GraphBuilderService {
    pub fn new(api_key: &str) -> Self {
        Self { client: ZepClient::new(api_key) }
    }

    pub async fn create_graph(&self, name: &str) -> anyhow::Result<String> {
        let graph_id = format!("mirofish_{}", &Uuid::new_v4().to_string().replace('-', "")[..16]);
        self.client.create_graph(&graph_id, name).await?;
        Ok(graph_id)
    }

    pub async fn set_ontology(&self, graph_id: &str, ontology: &Value) -> anyhow::Result<()> {
        self.client.set_ontology(graph_id, ontology).await
    }

    pub async fn add_text_batches(
        &self, graph_id: &str, chunks: &[String], batch_size: usize,
        progress_callback: Option<&(dyn Fn(&str, f64) + Send + Sync)>,
    ) -> anyhow::Result<Vec<String>> {
        let mut all_uuids = Vec::new();
        let total = chunks.len();

        for (i, batch) in chunks.chunks(batch_size).enumerate() {
            let batch_num = i + 1;
            let total_batches = (total + batch_size - 1) / batch_size;

            if let Some(cb) = &progress_callback {
                let progress = ((i * batch_size) + batch.len()) as f64 / total as f64;
                cb(&format!("Sending batch {}/{} ({} chunks)...", batch_num, total_batches, batch.len()), progress);
            }

            let batch_vec: Vec<String> = batch.to_vec();
            let uuids = self.client.add_text_batch(graph_id, &batch_vec).await?;
            all_uuids.extend(uuids);

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        Ok(all_uuids)
    }

    pub async fn wait_for_episodes(
        &self, episode_uuids: &[String],
        progress_callback: Option<&(dyn Fn(&str, f64) + Send + Sync)>,
    ) {
        if episode_uuids.is_empty() { return; }

        let total = episode_uuids.len();
        let mut pending: std::collections::HashSet<String> = episode_uuids.iter().cloned().collect();
        let mut completed = 0;
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(600);

        while !pending.is_empty() {
            if start.elapsed() > timeout { break; }

            let pending_list: Vec<String> = pending.iter().cloned().collect();
            for uuid in pending_list {
                if let Ok(processed) = self.client.check_episode_processed(&uuid).await {
                    if processed {
                        pending.remove(&uuid);
                        completed += 1;
                    }
                }
            }

            if let Some(cb) = &progress_callback {
                let progress = completed as f64 / total as f64;
                cb(&format!("Processing... {}/{} completed ({}s)", completed, total, start.elapsed().as_secs()), progress);
            }

            if !pending.is_empty() {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }

    pub async fn get_graph_data(&self, graph_id: &str) -> anyhow::Result<Value> {
        let nodes = self.client.get_nodes(graph_id).await?;
        let edges = self.client.get_edges(graph_id).await?;

        let node_map: std::collections::HashMap<String, String> = nodes.iter()
            .map(|n| (n.uuid.clone(), n.name.clone().unwrap_or_default()))
            .collect();

        let nodes_data: Vec<Value> = nodes.iter().map(|n| {
            serde_json::json!({
                "uuid": n.uuid,
                "name": n.name,
                "labels": n.labels,
                "summary": n.summary,
                "attributes": n.attributes,
                "created_at": n.created_at,
            })
        }).collect();

        let edges_data: Vec<Value> = edges.iter().map(|e| {
            serde_json::json!({
                "uuid": e.uuid,
                "name": e.name,
                "fact": e.fact,
                "fact_type": e.fact_type,
                "source_node_uuid": e.source_node_uuid,
                "target_node_uuid": e.target_node_uuid,
                "source_node_name": node_map.get(&e.source_node_uuid).unwrap_or(&String::new()),
                "target_node_name": node_map.get(&e.target_node_uuid).unwrap_or(&String::new()),
                "attributes": e.attributes,
                "created_at": e.created_at,
                "valid_at": e.valid_at,
                "invalid_at": e.invalid_at,
                "expired_at": e.expired_at,
                "episodes": e.episodes,
            })
        }).collect();

        Ok(serde_json::json!({
            "graph_id": graph_id,
            "nodes": nodes_data,
            "edges": edges_data,
            "node_count": nodes_data.len(),
            "edge_count": edges_data.len(),
        }))
    }

    pub async fn delete_graph(&self, graph_id: &str) -> anyhow::Result<()> {
        self.client.delete_graph(graph_id).await
    }
}
