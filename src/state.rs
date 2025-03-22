// graph_state.rs
use eframe::egui::{Pos2, Vec2};
use crate::graph::{Graph, ID, NodeData};
use slotmap::SecondaryMap;
use serde::{Serialize, Deserialize};
use std::io::{Read, Write};
use std::fs::File;
use std::collections::{HashSet, VecDeque};
use std::path::Path;

// Camera state to manage pan and zoom
#[derive(Serialize, Deserialize, Clone)]
pub struct Camera {
    pub offset: Vec2,     // Translation/pan offset
    pub zoom: f32,        // Zoom factor
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl Camera {
    // Convert world coordinates to screen coordinates
    pub fn world_to_screen(&self, world_pos: Pos2, screen_origin: Pos2) -> Pos2 {
        screen_origin + ((world_pos.to_vec2() + self.offset) * self.zoom)
    }
    
    // Convert screen coordinates to world coordinates
    pub fn screen_to_world(&self, screen_pos: Pos2, screen_origin: Pos2) -> Pos2 {
        let screen_vec = screen_pos - screen_origin;
        (screen_vec / self.zoom - self.offset).to_pos2()
    }
    
    // Reset to default (centered view, 1.0 zoom)
    pub fn reset(&mut self) {
        *self = Camera::default();
    }
}

// GraphState stores the graph, positions, and camera settings
#[derive(Serialize, Deserialize, Clone)]
pub struct GraphState {
    pub graph: Graph,
    pub positions: SecondaryMap<ID, Pos2>,
    pub camera: Camera,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            positions: SecondaryMap::new(),
            camera: Camera::default(),
        }
    }
}

impl GraphState {
    // Create a new empty graph state
    pub fn new() -> Self {
        Self::default()
    }
    
    // Recursively update midpoints for edge-nodes connected to `start_id`
    pub fn update_positions_recursive(&mut self, start_id: ID) {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start_id);

        while let Some(curr) = queue.pop_front() {
            if !visited.insert(curr) {
                continue;
            }
            if let Some(edge) = self.graph.get_edge(curr) {
                if let (Some(&src), Some(&tgt)) = (
                    self.positions.get(edge.source),
                    self.positions.get(edge.target),
                ) {
                    let mid = ((src.to_vec2() + tgt.to_vec2()) * 0.5).to_pos2();
                    self.positions.insert(curr, mid);
                }
            }
            let outgoing = self.graph.get_outgoing_edges(curr);
            let incoming = self.graph.get_incoming_edges(curr);
            for neighbor in outgoing.into_iter().chain(incoming) {
                queue.push_back(neighbor);
            }
        }
    }
    
    // Clean up positions that don't have corresponding graph elements
    pub fn cleanup_positions(&mut self) {
        self.positions.retain(|id, _| {
            self.graph.get_node(id).is_some() || self.graph.get_edge(id).is_some()
        });
    }
    
    // Add a new node at the given position
    pub fn add_node_at(&mut self, position: Pos2) -> ID {
        let node_id = self.graph.add_node(NodeData::default());
        self.positions.insert(node_id, position);
        node_id
    }
    
    // Remove a node or edge by ID
    pub fn remove_element(&mut self, id: ID) {
        if self.graph.get_node(id).is_some() {
            self.graph.remove_node(id);
        } else if self.graph.get_edge(id).is_some() {
            self.graph.remove_edge(id);
        }
        self.positions.remove(id);
    }
    
    // Add an edge between two nodes
    pub fn add_edge_between(&mut self, source: ID, target: ID) -> Option<ID> {
        self.graph.add_edge(source, target).inspect(|&edge_id| {
            // Calculate midpoint position for the edge
            if let (Some(&src_pos), Some(&tgt_pos)) = (
                self.positions.get(source),
                self.positions.get(target),
            ) {
                let mid = ((src_pos.to_vec2() + tgt_pos.to_vec2()) * 0.5).to_pos2();
                self.positions.insert(edge_id, mid);
            }
        })
    }
    
    // Find an element under the given position
    pub fn find_element_at(&self, position: Pos2, hit_radius: f32) -> Option<ID> {
        self.positions
            .iter()
            .find(|(_, &pos)| pos.distance(position) < hit_radius)
            .map(|(id, _)| id)
    }
    
    // Save the graph state to a file
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let encoded = bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let mut file = File::create(path)?;
        file.write_all(&encoded)?;
        Ok(())
    }
    
    // Load the graph state from a file
    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        let state: GraphState = bincode::deserialize(&buffer)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(state)
    }
}