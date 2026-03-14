use crate::services::zep_client::ZepClient;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNode {
    pub uuid: String,
    pub name: String,
    pub labels: Vec<String>,
    pub summary: String,
    pub attributes: Value,
    pub related_edges: Vec<Value>,
    pub related_nodes: Vec<Value>,
}

impl EntityNode {
    pub fn get_entity_type(&self) -> Option<String> {
        self.labels.iter()
            .find(|l| *l != "Entity" && *l != "Node")
            .cloned()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilteredEntities {
    pub entities: Vec<EntityNode>,
    pub entity_types: Vec<String>,
    pub total_count: usize,
    pub filtered_count: usize,
}

pub struct ZepEntityReader {
    client: ZepClient,
}

impl ZepEntityReader {
    pub fn new(api_key: &str) -> Self {
        Self { client: ZepClient::new(api_key) }
    }

    pub async fn filter_defined_entities(
        &self, graph_id: &str,
        defined_entity_types: Option<&[String]>,
        enrich_with_edges: bool,
    ) -> anyhow::Result<FilteredEntities> {
        let all_nodes = self.client.get_nodes(graph_id).await?;
        let total_count = all_nodes.len();

        let all_edges = if enrich_with_edges {
            self.client.get_edges(graph_id).await.unwrap_or_default()
        } else {
            vec![]
        };

        let node_map: HashMap<String, &crate::services::zep_client::ZepNode> = all_nodes.iter()
            .map(|n| (n.uuid.clone(), n))
            .collect();

        let mut filtered = Vec::new();
        let mut entity_types_found = HashSet::new();

        for node in &all_nodes {
            let labels = node.labels.clone().unwrap_or_default();
            let custom_labels: Vec<String> = labels.iter()
                .filter(|l| *l != "Entity" && *l != "Node")
                .cloned()
                .collect();

            if custom_labels.is_empty() { continue; }

            let entity_type = if let Some(defined) = defined_entity_types {
                let matching: Vec<&String> = custom_labels.iter()
                    .filter(|l| defined.contains(l))
                    .collect();
                if matching.is_empty() { continue; }
                matching[0].clone()
            } else {
                custom_labels[0].clone()
            };

            entity_types_found.insert(entity_type);

            let mut related_edges = Vec::new();
            let mut related_node_uuids = HashSet::new();

            if enrich_with_edges {
                for edge in &all_edges {
                    if edge.source_node_uuid == node.uuid {
                        related_edges.push(serde_json::json!({
                            "direction": "outgoing",
                            "edge_name": edge.name,
                            "fact": edge.fact,
                            "target_node_uuid": edge.target_node_uuid,
                        }));
                        related_node_uuids.insert(edge.target_node_uuid.clone());
                    } else if edge.target_node_uuid == node.uuid {
                        related_edges.push(serde_json::json!({
                            "direction": "incoming",
                            "edge_name": edge.name,
                            "fact": edge.fact,
                            "source_node_uuid": edge.source_node_uuid,
                        }));
                        related_node_uuids.insert(edge.source_node_uuid.clone());
                    }
                }
            }

            let related_nodes: Vec<Value> = related_node_uuids.iter()
                .filter_map(|uuid| node_map.get(uuid))
                .map(|n| serde_json::json!({
                    "uuid": n.uuid,
                    "name": n.name,
                    "labels": n.labels,
                    "summary": n.summary,
                }))
                .collect();

            filtered.push(EntityNode {
                uuid: node.uuid.clone(),
                name: node.name.clone().unwrap_or_default(),
                labels,
                summary: node.summary.clone().unwrap_or_default(),
                attributes: node.attributes.clone().unwrap_or(serde_json::json!({})),
                related_edges,
                related_nodes,
            });
        }

        Ok(FilteredEntities {
            filtered_count: filtered.len(),
            entities: filtered,
            entity_types: entity_types_found.into_iter().collect(),
            total_count,
        })
    }

    pub async fn get_entity_with_context(
        &self, graph_id: &str, entity_uuid: &str,
    ) -> anyhow::Result<Option<EntityNode>> {
        let node = match self.client.get_node(entity_uuid).await {
            Ok(n) => n,
            Err(_) => return Ok(None),
        };

        let all_nodes = self.client.get_nodes(graph_id).await.unwrap_or_default();
        let node_map: HashMap<String, &crate::services::zep_client::ZepNode> = all_nodes.iter()
            .map(|n| (n.uuid.clone(), n))
            .collect();

        let edges = self.client.get_node_edges(entity_uuid).await.unwrap_or_default();

        let mut related_edges = Vec::new();
        let mut related_node_uuids = HashSet::new();

        for edge in &edges {
            if edge.source_node_uuid == entity_uuid {
                related_edges.push(serde_json::json!({
                    "direction": "outgoing",
                    "edge_name": edge.name,
                    "fact": edge.fact,
                    "target_node_uuid": edge.target_node_uuid,
                }));
                related_node_uuids.insert(edge.target_node_uuid.clone());
            } else {
                related_edges.push(serde_json::json!({
                    "direction": "incoming",
                    "edge_name": edge.name,
                    "fact": edge.fact,
                    "source_node_uuid": edge.source_node_uuid,
                }));
                related_node_uuids.insert(edge.source_node_uuid.clone());
            }
        }

        let related_nodes: Vec<Value> = related_node_uuids.iter()
            .filter_map(|uuid| node_map.get(uuid))
            .map(|n| serde_json::json!({
                "uuid": n.uuid,
                "name": n.name,
                "labels": n.labels,
                "summary": n.summary,
            }))
            .collect();

        Ok(Some(EntityNode {
            uuid: node.uuid,
            name: node.name.unwrap_or_default(),
            labels: node.labels.unwrap_or_default(),
            summary: node.summary.unwrap_or_default(),
            attributes: node.attributes.unwrap_or(serde_json::json!({})),
            related_edges,
            related_nodes,
        }))
    }
}
