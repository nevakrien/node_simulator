use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, DenseSlotMap,SecondaryMap};
use std::collections::{HashMap, HashSet};

new_key_type! {
    pub struct ID;
}

#[derive(Default,Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeData{}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    id: ID,
    data: NodeData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    id: ID,           // Graph element ID (can be connected to like a node)
    source: ID,
    target: ID,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Graph {
    nodes: DenseSlotMap<ID, Node>,
    edges: SecondaryMap<ID, Edge>,
    
    // Maps node/edge ID to its outgoing edges
    source_to_edges: HashMap<ID, HashSet<ID>>,
    
    // Maps node/edge ID to its incoming edges
    target_to_edges: HashMap<ID, HashSet<ID>>,
    
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_node(&mut self, data: NodeData) -> ID {
        self.nodes.insert_with_key(|k| Node { id: k, data })
    }
    
    pub fn remove_node(&mut self, id: ID) -> Option<Node> {
        // Can't remove a node that doesn't exist
        if !self.nodes.contains_key(id) {
            return None;
        }
        
        // Collect all edges to remove
        let mut edges_to_remove = Vec::new();
        
        // Add outgoing edges
        if let Some(outgoing) = self.source_to_edges.get(&id) {
            edges_to_remove.extend(outgoing.iter().copied());
        }
        
        // Add incoming edges
        if let Some(incoming) = self.target_to_edges.get(&id) {
            edges_to_remove.extend(incoming.iter().copied());
        }
        
        // Remove all connected edges
        for edge_id in edges_to_remove {
            self.remove_edge(edge_id);
        }
        
        // Remove the node from the lookup maps
        self.source_to_edges.remove(&id);
        self.target_to_edges.remove(&id);
        
        // Remove the node
        self.nodes.remove(id)
    }
    
    pub fn add_edge(&mut self, source: ID, target: ID) -> Option<ID> {
        // Check if both endpoints exist (either as nodes or as edges)
        let source_exists = self.nodes.contains_key(source);
        let target_exists = self.nodes.contains_key(target);
        
        if !source_exists || !target_exists {
            return None;
        }
        
        // Create a temporary ID for the graph element
        let id = self.nodes.insert_with_key(|k| Node { 
            id: k, 
            data: NodeData::default() 
        });
        
        
        self.edges.insert(id,Edge {
            id,
            source,
            target,
        });


        
        
        // Update source_to_edges map
        self.source_to_edges
            .entry(source)
            .or_insert_with(HashSet::new)
            .insert(id);
        
        // Update target_to_edges map
        self.target_to_edges
            .entry(target)
            .or_insert_with(HashSet::new)
            .insert(id);
        
        Some(id)
    }
    
    pub fn remove_edge(&mut self, edge_id: ID) -> Option<Edge> {
        // Get the edge
        let edge = self.edges.get(edge_id)?;
        let graph_id = edge.id;
        let source = edge.source;
        let target = edge.target;
        
        // Collect all child edges that need to be removed
        let mut child_edges = Vec::new();
        if let Some(outgoing) = self.source_to_edges.get(&graph_id) {
            for &child_edge_id in outgoing {
                if let Some(edge) = self.edges.get(child_edge_id) {
                    child_edges.push(edge.id);
                }
            }
        }
        
        // Remove all child edges
        for child_id in child_edges {
            self.remove_edge(child_id);
        }
        
        // Remove the edge from source_to_edges
        if let Some(edges) = self.source_to_edges.get_mut(&source) {
            edges.remove(&edge_id);
            if edges.is_empty() {
                self.source_to_edges.remove(&source);
            }
        }
        
        // Remove the edge from target_to_edges
        if let Some(edges) = self.target_to_edges.get_mut(&target) {
            edges.remove(&edge_id);
            if edges.is_empty() {
                self.target_to_edges.remove(&target);
            }
        }
        
        
        // Remove and return the edge
        self.edges.remove(edge_id)
    }
    
    pub fn get_node(&self, id: ID) -> Option<&Node> {
        self.nodes.get(id)
    }
    
    pub fn get_edge(&self, id: ID) -> Option<&Edge> {
        self.edges.get(id)
    }
    
    pub fn get_outgoing_edges(&self, id: ID) -> Vec<ID> {
        match self.source_to_edges.get(&id) {
            Some(edge_ids) => {
                edge_ids.iter()
                    .filter_map(|&edge_id| {
                        self.edges.get(edge_id).map(|edge| edge.id)
                    })
                    .collect()
            }
            None => Vec::new(),
        }
    }
    
    pub fn get_incoming_edges(&self, id: ID) -> Vec<ID> {
        match self.target_to_edges.get(&id) {
            Some(edge_ids) => {
                edge_ids.iter()
                    .filter_map(|&edge_id| {
                        self.edges.get(edge_id).map(|edge| edge.id)
                    })
                    .collect()
            }
            None => Vec::new(),
        }
    }
    
    pub fn nodes_iter(&self) -> slotmap::dense::Iter<'_, ID, Node> {
        self.nodes.iter()
    }
    
    pub fn edges_iter(&self) -> impl Iterator<Item = &Edge> {
        self.edges.values()
    }
    
    
    // Serialization to binary (using bincode)
    pub fn to_binary(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }
    
    // Deserialization from binary
    pub fn from_binary(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_remove_nodes() {
        let mut graph = Graph::new();
        
        let node1 = graph.add_node(NodeData::default());
        let node2 = graph.add_node(NodeData::default());
        
        assert!(graph.get_node(node1).is_some());
        assert!(graph.get_node(node2).is_some());
        
        let removed = graph.remove_node(node1);
        assert!(removed.is_some());
        assert!(graph.get_node(node1).is_none());
        assert!(graph.get_node(node2).is_some());
    }
    

    #[test]
    fn test_add_remove_edges() {
        let mut graph = Graph::new();
        
        let node1 = graph.add_node(NodeData::default());
        let node2 = graph.add_node(NodeData::default());
        
        // Test adding an edge
        let edge_id = graph.add_edge(node1, node2).unwrap();
        
        let edge = graph.get_edge(edge_id);
        assert!(edge.is_some());
        
        let edge = edge.unwrap();
        assert_eq!(edge.source, node1);
        assert_eq!(edge.target, node2);
        
        // Test adding an edge with non-existent source
        let non_existent_id = ID::default(); // This ID doesn't exist in the graph
        assert!(graph.add_edge(non_existent_id, node2).is_none());
        
        // Test adding an edge with non-existent target
        assert!(graph.add_edge(node1, non_existent_id).is_none());
        
        // Test removing edge
        let removed_edge = graph.remove_edge(edge_id);
        assert!(removed_edge.is_some());
        assert!(graph.get_edge(edge_id).is_none());
        
        // Test removing the same edge twice (should return None)
        let removed_again = graph.remove_edge(edge_id);
        assert!(removed_again.is_none());
        
        // Add edge again
        let edge_id = graph.add_edge(node1, node2).unwrap();
        
        // Remove node - should cascade remove the edge
        graph.remove_node(node1);
        assert!(graph.get_edge(edge_id).is_none());
    }
    
    #[test]
    fn test_edge_to_edge_connection() {
        let mut graph = Graph::new();
        
        let node1 = graph.add_node(NodeData::default());
        let node2 = graph.add_node(NodeData::default());
        
        // Create an edge between nodes
        let edge1 = graph.add_edge(node1, node2).unwrap();
        
        // Create second-order edge (edge connecting to another edge)
        let edge2 = graph.add_edge(edge1, node2).unwrap();
        
        // Create third-order edge (edge connecting to an edge that connects to another edge)
        let edge3 = graph.add_edge(edge2, node1).unwrap();
        
        // Check all edges exist
        assert!(graph.get_edge(edge1).is_some());
        assert!(graph.get_edge(edge2).is_some());
        assert!(graph.get_edge(edge3).is_some());
        
        // Check the edge connections
        let outgoing_from_edge1 = graph.get_outgoing_edges(edge1);
        assert_eq!(outgoing_from_edge1.len(), 1);
        assert!(outgoing_from_edge1.contains(&edge2));
        
        let outgoing_from_edge2 = graph.get_outgoing_edges(edge2);
        assert_eq!(outgoing_from_edge2.len(), 1);
        assert!(outgoing_from_edge2.contains(&edge3));
        
        // Test removing the middle edge (edge2)
        // This should also remove edge3 due to cascade
        graph.remove_edge(edge2);
        assert!(graph.get_edge(edge2).is_none());
        assert!(graph.get_edge(edge3).is_none());
        assert!(graph.get_edge(edge1).is_some()); // edge1 should still exist
        
        // Test circular reference
        let edge4 = graph.add_edge(edge1, node2).unwrap();
        let edge5 = graph.add_edge(edge4, edge1).unwrap();
        
        // Check the circular reference
        let outgoing_from_edge4 = graph.get_outgoing_edges(edge4);
        assert!(outgoing_from_edge4.contains(&edge5));
        
        assert_eq!(graph.get_edge(edge5).unwrap().target, edge1);
        
        // Removing edge1 should remove both edge4 and edge5 due to cascade
        graph.remove_edge(edge1);
        assert!(graph.get_edge(edge1).is_none());
        assert!(graph.get_edge(edge4).is_none());
        assert!(graph.get_edge(edge5).is_none());
    }
    
    #[test]
    fn test_complex_cascading_removal() {
        let mut graph = Graph::new();
        
        // Create some nodes
        let node1 = graph.add_node(NodeData::default());
        let node2 = graph.add_node(NodeData::default());
        let node3 = graph.add_node(NodeData::default());
        
        // Create a chain of edges: node1 -> edge1 -> edge2 -> edge3 -> node3
        let edge1 = graph.add_edge(node1, node2).unwrap();
        let edge2 = graph.add_edge(edge1, node2).unwrap();
        let edge3 = graph.add_edge(edge2, node3).unwrap();
        
        // Create a branch: edge2 -> edge4 -> node3
        let edge4 = graph.add_edge(edge2, node3).unwrap();
        
        // Remove edge2 - should cascade to edge3 and edge4
        graph.remove_edge(edge2);
        assert!(graph.get_edge(edge2).is_none());
        assert!(graph.get_edge(edge3).is_none());
        assert!(graph.get_edge(edge4).is_none());
        assert!(graph.get_edge(edge1).is_some()); // edge1 should remain
        
        // Test removing a node that's part of multiple edge connections
        let edge5 = graph.add_edge(node1, node2).unwrap();
        let edge6 = graph.add_edge(node2, node3).unwrap();
        let edge7 = graph.add_edge(edge5, edge6).unwrap();
        
        // Remove node2 - should cascade to all connected edges
        graph.remove_node(node2);
        assert!(graph.get_edge(edge5).is_none());
        assert!(graph.get_edge(edge6).is_none());
        assert!(graph.get_edge(edge7).is_none());
    }
   
    #[test]
    fn test_node_connections() {
        let mut graph = Graph::new();
        
        let node1 = graph.add_node(NodeData::default());
        let node2 = graph.add_node(NodeData::default());
        let node3 = graph.add_node(NodeData::default());
        
        let edge1 = graph.add_edge(node1, node2).unwrap();
        let edge2 = graph.add_edge(node1, node3).unwrap();
        let edge3 = graph.add_edge(node2, node3).unwrap();
        
        let outgoing_from_1 = graph.get_outgoing_edges(node1);
        assert_eq!(outgoing_from_1.len(), 2);
        assert!(outgoing_from_1.contains(&edge1));
        assert!(outgoing_from_1.contains(&edge2));
        
        let incoming_to_3 = graph.get_incoming_edges(node3);
        assert_eq!(incoming_to_3.len(), 2);
        assert!(incoming_to_3.contains(&edge2));
        assert!(incoming_to_3.contains(&edge3));
    }
    
    #[test]
    fn test_serialization() {
        let mut graph = Graph::new();
        
        let node1 = graph.add_node(NodeData::default());
        let node2 = graph.add_node(NodeData::default());
        
        graph.add_edge(node1, node2);
        
        // Test binary serialization
        let binary = graph.to_binary().unwrap();
        let deserialized = Graph::from_binary(&binary).unwrap();
        
        assert!(deserialized.get_node(node1).is_some());
        assert!(deserialized.get_node(node2).is_some());

    }
}