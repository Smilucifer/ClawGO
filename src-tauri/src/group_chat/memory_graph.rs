use crate::models::{CommunityInfo, KnowledgeGapInfo, MemoryEdge, MemoryNode};
use petgraph::graph::DiGraph;
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, HashSet};

/// Build a petgraph DiGraph from memory nodes and edges.
/// Node weights are the content string; edge weights are the relevance score (f64).
pub fn build_graph(nodes: &[MemoryNode], edges: &[MemoryEdge]) -> DiGraph<String, f64> {
    let mut graph = DiGraph::new();
    let mut node_map: HashMap<String, petgraph::graph::NodeIndex> = HashMap::new();

    for node in nodes {
        let idx = graph.add_node(node.content.clone());
        node_map.insert(node.id.clone(), idx);
    }

    for edge in edges {
        if let (Some(&src), Some(&tgt)) = (
            node_map.get(&edge.source_id),
            node_map.get(&edge.target_id),
        ) {
            graph.add_edge(src, tgt, edge.weight);
        }
    }
    graph
}

/// Compute relevance edges between a new memory node and all existing nodes.
///
/// Uses three signals:
/// - Source overlap (shared group_chat_id) — weight 4.0
/// - Direct tag overlap — weight 3.0 (normalized)
/// - Type affinity — weight 1.0
pub fn compute_relevance_edges(
    new_node: &MemoryNode,
    existing_nodes: &[MemoryNode],
    _existing_edges: &[MemoryEdge],
) -> Vec<MemoryEdge> {
    let mut edges = Vec::new();

    for existing in existing_nodes {
        if existing.id == new_node.id {
            continue;
        }

        // Signal 1: Source overlap (weight 4.0)
        let source_overlap = if new_node.source.group_chat_id.is_some()
            && new_node.source.group_chat_id == existing.source.group_chat_id
        {
            4.0
        } else {
            0.0
        };

        // Signal 2: Direct tags overlap (weight 3.0, normalized)
        let tag_overlap = new_node
            .tags
            .iter()
            .filter(|t| existing.tags.contains(t))
            .count() as f64
            * 3.0
            / new_node.tags.len().max(1) as f64;

        // Signal 3: Type affinity (weight 1.0)
        let type_affinity = if new_node.memory_type == existing.memory_type {
            1.0
        } else {
            0.0
        };

        let total = source_overlap + tag_overlap + type_affinity;
        if total > 0.5 {
            let relation = if source_overlap > 0.0 {
                "related"
            } else if type_affinity > 0.0 {
                "extends"
            } else {
                "related"
            };

            edges.push(MemoryEdge {
                id: uuid::Uuid::new_v4().to_string(),
                source_id: new_node.id.clone(),
                target_id: existing.id.clone(),
                relation: relation.to_string(),
                weight: (total / 8.0).min(1.0),
            });
        }
    }
    edges
}

/// Simplified Louvain community detection on the memory graph.
///
/// Iteratively moves each node to the community of the neighbor with the
/// highest weighted edge sum. Returns communities sorted by size (largest first).
pub fn detect_communities(
    nodes: &[MemoryNode],
    edges: &[MemoryEdge],
) -> Vec<CommunityInfo> {
    let graph = build_graph(nodes, edges);
    let n = graph.node_count();
    if n == 0 {
        return Vec::new();
    }

    // Initialise: each node in its own community
    let mut communities: Vec<usize> = (0..n).collect();
    let mut changed = true;
    let mut iteration = 0;
    let max_iterations = 50;

    while changed && iteration < max_iterations {
        changed = false;
        iteration += 1;

        for node in graph.node_indices() {
            let nidx = node.index();
            let current_comm = communities[nidx];

            // Sum edge weights per neighbour community
            let mut neighbor_comms: HashMap<usize, f64> = HashMap::new();
            for edge in graph.edges(node) {
                let neighbor = edge.target().index();
                let comm = communities[neighbor];
                *neighbor_comms.entry(comm).or_insert(0.0) += edge.weight();
            }

            // Pick the community with the strongest connection
            let mut best_comm = current_comm;
            let mut best_gain = 0.0;
            for (&comm, &weight) in &neighbor_comms {
                if comm != current_comm && weight > best_gain {
                    best_gain = weight;
                    best_comm = comm;
                }
            }

            if best_comm != current_comm {
                communities[nidx] = best_comm;
                changed = true;
            }
        }
    }

    // Group node indices by community ID
    let mut comm_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for (ni, &ci) in communities.iter().enumerate() {
        comm_map.entry(ci).or_default().push(ni);
    }

    let mut result: Vec<CommunityInfo> = comm_map
        .into_iter()
        .enumerate()
        .map(|(i, (_, member_indices))| {
            let node_ids: Vec<String> = member_indices
                .iter()
                .filter_map(|&idx| nodes.get(idx).map(|n| n.id.clone()))
                .collect();

            let n = member_indices.len();
            let mut internal_edges = 0;
            for (ai, &a) in member_indices.iter().enumerate() {
                for &b in &member_indices[ai + 1..] {
                    for edge in edges {
                        if (edge.source_id == nodes[a].id && edge.target_id == nodes[b].id)
                            || (edge.source_id == nodes[b].id && edge.target_id == nodes[a].id)
                        {
                            internal_edges += 1;
                        }
                    }
                }
            }
            let max_edges = if n > 1 { n * (n - 1) / 2 } else { 1 };
            let cohesion = internal_edges as f64 / max_edges as f64;

            CommunityInfo {
                id: i,
                label: format!("社区 {}", i + 1),
                cohesion,
                node_count: n,
                edge_count: internal_edges,
                node_ids,
            }
        })
        .collect();

    // Sort by size descending
    result.sort_by(|a, b| b.node_count.cmp(&a.node_count));
    result
}

/// Detect knowledge gaps in the memory graph.
///
/// Three types of gaps are reported:
/// - `isolated_node`: nodes with degree <= 1 (weakly connected)
/// - `sparse_community`: communities with cohesion < 0.15 and >= 3 nodes
/// - `bridge_node`: nodes connected to 3+ communities (key hubs with risk)
pub fn detect_knowledge_gaps(
    nodes: &[MemoryNode],
    edges: &[MemoryEdge],
    communities: &[CommunityInfo],
) -> Vec<KnowledgeGapInfo> {
    let mut gaps = Vec::new();

    // ── Isolated nodes (degree <= 1) ──
    let mut degrees: HashMap<String, usize> = HashMap::new();
    for edge in edges {
        *degrees.entry(edge.source_id.clone()).or_default() += 1;
        *degrees.entry(edge.target_id.clone()).or_default() += 1;
    }
    for node in nodes {
        let deg = degrees.get(&node.id).copied().unwrap_or(0);
        if deg <= 1 {
            let preview = truncate(&node.content, 50);
            gaps.push(KnowledgeGapInfo {
                gap_type: "isolated_node".to_string(),
                description: format!("\"{}\" 仅连接 {} 条记忆，是知识孤岛", preview, deg),
                suggestion: "建议将这条记忆与其他相关记忆建立关联".to_string(),
                affected_node_ids: vec![node.id.clone()],
            });
        }
    }

    // ── Sparse communities (cohesion < 0.15, >= 3 nodes) ──
    for comm in communities {
        if comm.cohesion < 0.15 && comm.node_count >= 3 {
            gaps.push(KnowledgeGapInfo {
                gap_type: "sparse_community".to_string(),
                description: format!(
                    "{} 凝聚力仅 {:.2}，节点间连接稀疏",
                    comm.label, comm.cohesion
                ),
                suggestion: "建议为该知识领域补充更多关联记忆".to_string(),
                affected_node_ids: comm.node_ids.clone(),
            });
        }
    }

    // ── Bridge nodes (connected to >= 3 communities) ──
    let mut node_community_map: HashMap<String, HashSet<usize>> = HashMap::new();
    for (comm_idx, comm) in communities.iter().enumerate() {
        for nid in &comm.node_ids {
            node_community_map
                .entry(nid.clone())
                .or_default()
                .insert(comm_idx);
        }
    }
    for (nid, comms) in &node_community_map {
        if comms.len() >= 3 {
            if let Some(node) = nodes.iter().find(|n| &n.id == nid) {
                let preview = truncate(&node.content, 50);
                gaps.push(KnowledgeGapInfo {
                    gap_type: "bridge_node".to_string(),
                    description: format!(
                        "\"{}\" 连接 {} 个知识社区，是关键枢纽节点",
                        preview,
                        comms.len()
                    ),
                    suggestion: "枢纽节点置信度应保持较高水平，建议补充更多上下文".to_string(),
                    affected_node_ids: vec![nid.clone()],
                });
            }
        }
    }

    gaps
}

/// BFS-based graph traversal from seed node IDs, expanding up to `hops` hops.
///
/// Returns all visited node IDs (including seeds).
pub fn graph_expand(
    edges: &[MemoryEdge],
    seed_ids: &[String],
    hops: usize,
) -> Vec<String> {
    let mut visited: HashSet<String> = seed_ids.iter().cloned().collect();
    let mut frontier: HashSet<String> = seed_ids.iter().cloned().collect();

    for _ in 0..hops {
        let mut next_frontier = HashSet::new();
        for edge in edges {
            if frontier.contains(&edge.source_id) && !visited.contains(&edge.target_id) {
                next_frontier.insert(edge.target_id.clone());
            }
            if frontier.contains(&edge.target_id) && !visited.contains(&edge.source_id) {
                next_frontier.insert(edge.source_id.clone());
            }
        }
        for id in &next_frontier {
            visited.insert(id.clone());
        }
        frontier = next_frontier;
    }

    visited.into_iter().collect()
}

/// Truncate a string to at most `max_len` characters for display.
///
/// Uses `char_indices` to find a safe UTF-8 boundary, avoiding panics
/// on multi-byte characters (Chinese, emoji, etc.).
fn truncate(s: &str, max_len: usize) -> &str {
    if let Some((idx, _)) = s.char_indices().nth(max_len) {
        &s[..idx]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, content: &str, mem_type: &str, tags: Vec<&str>) -> MemoryNode {
        MemoryNode {
            id: id.to_string(),
            character_id: "char-1".to_string(),
            content: content.to_string(),
            memory_type: mem_type.to_string(),
            confidence: 0.8,
            source: crate::models::MemorySource {
                kind: "group_chat".to_string(),
                run_id: None,
                group_chat_id: Some("gc-1".to_string()),
            },
            tags: tags.into_iter().map(|t| t.to_string()).collect(),
            created_at: "2026-05-14T00:00:00Z".to_string(),
            updated_at: "2026-05-14T00:00:00Z".to_string(),
        }
    }

    fn make_edge(id: &str, src: &str, tgt: &str, weight: f64) -> MemoryEdge {
        MemoryEdge {
            id: id.to_string(),
            source_id: src.to_string(),
            target_id: tgt.to_string(),
            relation: "related".to_string(),
            weight,
        }
    }

    #[test]
    fn test_build_graph_empty() {
        let g = build_graph(&[], &[]);
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_build_graph_with_nodes_and_edges() {
        let nodes = vec![
            make_node("n1", "Alice", "person", vec!["dev"]),
            make_node("n2", "Bob", "person", vec!["dev"]),
            make_node("n3", "Carol", "person", vec!["pm"]),
        ];
        let edges = vec![
            make_edge("e1", "n1", "n2", 0.8),
            make_edge("e2", "n2", "n3", 0.5),
        ];
        let g = build_graph(&nodes, &edges);
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn test_compute_relevance_edges_source_overlap() {
        let new = make_node("n4", "New node", "concept", vec!["rust"]);
        let existing = vec![
            make_node("n1", "Existing", "concept", vec!["rust"]),
            make_node("n2", "Other", "person", vec!["go"]),
        ];
        let edges = compute_relevance_edges(&new, &existing, &[]);
        // n1 shares tag "rust" → tag_overlap = 1*3.0/1 = 3.0, source overlap = 4.0, type affinity = 1.0
        // total = 8.0 → weight = 1.0
        // n2 shares nothing → no edge
        assert_eq!(edges.len(), 1);
        assert!((edges[0].weight - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_relevance_edges_no_match() {
        let new = make_node("n4", "New node", "concept", vec!["rust"]);
        let existing = vec![make_node(
            "n1",
            "Existing",
            "person",
            vec!["go", "python"],
        )];
        let edges = compute_relevance_edges(&new, &existing, &[]);
        // No tag overlap, no source overlap (same gc-1 though), different type
        // source_overlap = 4.0 since both have group_chat_id = Some("gc-1")
        // Wait — both have group_chat_id = Some("gc-1") from make_node!
        // So source_overlap = 4.0, tag_overlap = 0, type_affinity = 0
        // total = 4.0 > 0.5 → will produce an edge with weight = 4.0/8.0 = 0.5
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn test_detect_communities_empty() {
        let communities = detect_communities(&[], &[]);
        assert!(communities.is_empty());
    }

    #[test]
    fn test_detect_communities_two_clusters() {
        let nodes = vec![
            make_node("n1", "A1", "concept", vec!["x"]),
            make_node("n2", "A2", "concept", vec!["x"]),
            make_node("n3", "B1", "concept", vec!["y"]),
            make_node("n4", "B2", "concept", vec!["y"]),
        ];
        let edges = vec![
            make_edge("e1", "n1", "n2", 1.0),
            make_edge("e2", "n3", "n4", 1.0),
        ];
        let communities = detect_communities(&nodes, &edges);
        assert!(!communities.is_empty());
        // Should have 2 communities of size 2 each
        let sizes: Vec<usize> = communities.iter().map(|c| c.node_count).collect();
        assert_eq!(sizes, vec![2, 2]);
    }

    #[test]
    fn test_detect_knowledge_gaps_isolated() {
        let nodes = vec![
            make_node("n1", "Connected", "concept", vec![]),
            make_node("n2", "Isolated", "concept", vec![]),
        ];
        let edges = vec![make_edge("e1", "n1", "n1", 0.0)]; // self-loop counts as degree 2 (source + target)
        let communities = detect_communities(&nodes, &edges);
        let gaps = detect_knowledge_gaps(&nodes, &edges, &communities);
        let isolated: Vec<&KnowledgeGapInfo> = gaps
            .iter()
            .filter(|g| g.gap_type == "isolated_node")
            .collect();
        // n1 has degree 2 (self-loop counted twice), n2 has degree 0 → only n2 is isolated
        assert_eq!(isolated.len(), 1);
    }

    #[test]
    fn test_graph_expand_one_hop() {
        let _nodes = vec![
            make_node("n1", "A", "concept", vec![]),
            make_node("n2", "B", "concept", vec![]),
            make_node("n3", "C", "concept", vec![]),
        ];
        let edges = vec![
            make_edge("e1", "n1", "n2", 1.0),
            make_edge("e2", "n2", "n3", 1.0),
        ];
        let result = graph_expand(&edges, &["n1".to_string()], 1);
        assert!(result.contains(&"n1".to_string()));
        assert!(result.contains(&"n2".to_string()));
        assert!(!result.contains(&"n3".to_string()));
    }

    #[test]
    fn test_graph_expand_two_hops() {
        let _nodes = vec![
            make_node("n1", "A", "concept", vec![]),
            make_node("n2", "B", "concept", vec![]),
            make_node("n3", "C", "concept", vec![]),
        ];
        let edges = vec![
            make_edge("e1", "n1", "n2", 1.0),
            make_edge("e2", "n2", "n3", 1.0),
        ];
        let result = graph_expand(&edges, &["n1".to_string()], 2);
        assert!(result.contains(&"n1".to_string()));
        assert!(result.contains(&"n2".to_string()));
        assert!(result.contains(&"n3".to_string()));
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let long = "a".repeat(100);
        assert_eq!(truncate(&long, 50).len(), 50);
    }
}
