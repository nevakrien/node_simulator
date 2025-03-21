mod graph;

use crate::egui::Vec2;
use eframe::{egui, App, Frame};
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind};
use graph::{Graph, ID, NodeData};
use slotmap::{Key, SecondaryMap};
use std::collections::{HashSet, VecDeque};

struct GraphEditor {
    graph: Graph,
    /// Stores on-screen positions for both nodes and edge-nodes.
    positions: SecondaryMap<ID, Pos2>,
    /// Currently selected element (if any). This can be a node or an edge-node.
    selected: Option<ID>,
    /// If set, holds the source node for creating an edge.
    edge_mode: Option<ID>,

    /// If we're dragging a node, store its ID here.
    dragging: Option<ID>,
    /// The accumulated pointer delta or offset used for dragging.
    drag_offset: Option<egui::Vec2>,
}

impl Default for GraphEditor {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            positions: SecondaryMap::new(),
            selected: None,
            edge_mode: None,
            dragging: None,
            drag_offset: None,
        }
    }
}

impl GraphEditor {
    /// Clean up the positions cache by removing IDs that no longer exist in the graph.
    fn cleanup_positions(&mut self) {
        self.positions
            .retain(|id, _| self.graph.get_node(id).is_some() || self.graph.get_edge(id).is_some());
    }

    /// Recursively update the positions of edge-nodes connected (directly or indirectly) to `start_id`.
    ///
    /// We do a BFS/DFS from `start_id`, traversing edges in both directions. Whenever we encounter an edge node,
    /// we recompute its midpoint from its source & target. Then we continue outward from that edge to any edges
    /// that connect to it, etc. This ensures that all nested/cascading edge connections remain accurate.
    fn update_positions_recursive(&mut self, start_id: ID) {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start_id);

        while let Some(curr) = queue.pop_front() {
            if !visited.insert(curr) {
                continue; // already visited
            }

            // If curr is an edge-node, recompute its midpoint based on source and target
            if let Some(edge) = self.graph.get_edge(curr) {
                // If either endpoint was removed or doesn't have a position, skip
                if let (Some(&src_pos), Some(&tgt_pos)) = (
                    self.positions.get(edge.source),
                    self.positions.get(edge.target),
                ) {
                    let midpoint = ((src_pos.to_vec2() + tgt_pos.to_vec2()) * 0.5).to_pos2();
                    self.positions.insert(curr, midpoint);
                }
            }

            // Enqueue neighbors (outgoing & incoming edges)
            let outgoing = self.graph.get_outgoing_edges(curr);
            let incoming = self.graph.get_incoming_edges(curr);
            for neighbor in outgoing.into_iter().chain(incoming) {
                queue.push_back(neighbor);
            }
        }
    }
}

impl App for GraphEditor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Left-click on empty space: create a node");
            ui.label("Shift + Left-click on a node: create an edge (click two nodes)");
            ui.label("Right-click on a node/edge: delete it");
            ui.label("Left-click and drag a real node: move it (edges update automatically)");

            // Allocate a painter for drawing.
            let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::drag());

            // Helper: convert a local position to screen coordinates.
            let to_screen = |pos: Pos2| -> Pos2 {
                response.rect.left_top() + pos.to_vec2()
            };

            // -----------------------------------------------------------
            // 1) DRAG LOGIC: if we're currently dragging, update position
            // -----------------------------------------------------------
            if let Some(dragging_id) = self.dragging {
                // If user let go of the primary button, stop dragging
                if !ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                    self.dragging = None;
                    self.drag_offset = None;
                    // After finishing the drag, recursively update edges
                    self.update_positions_recursive(dragging_id);
                } else {
                    // Keep dragging
                    let pointer_delta = ctx.input(|i| i.pointer.delta());
                    if pointer_delta != Vec2::new(0.0,0.0) {
                        if let Some(old_pos) = self.positions.get(dragging_id) {
                            let new_pos = *old_pos + pointer_delta;
                            self.positions.insert(dragging_id, new_pos);
                        }
                    }
                }
            }

            // -----------------------------------------------------------
            // 2) Handle pointer input for click, new drag, edge creation, deletion, etc.
            // -----------------------------------------------------------
            if ctx.input(|i| i.pointer.any_pressed()) {
                if let Some(click_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    let local_pos = (click_pos - response.rect.left_top()).to_pos2();

                    // Check if click is near an existing element.
                    let clicked_id = self
                        .positions
                        .iter()
                        .find(|(_, &pos)| pos.distance(local_pos) < 20.0)
                        .map(|(id, _)| id);

                    // If we found an existing item under the pointer:
                    if let Some(id) = clicked_id {
                        // 2a) Right-click => Delete
                        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
                            if self.graph.get_node(id).is_some() {
                                self.graph.remove_node(id);
                            } else if self.graph.get_edge(id).is_some() {
                                self.graph.remove_edge(id);
                            }
                            self.positions.remove(id);
                        }
                        // 2b) Shift+Left-click => Create Edge
                        else if ctx.input(|i| {
                            i.modifiers.shift
                                && i.pointer.button_pressed(egui::PointerButton::Primary)
                        }) {
                            if let Some(source) = self.edge_mode.take() {
                                if let Some(edge_id) = self.graph.add_edge(source, id) {
                                    // Compute the midpoint for the new edge-node.
                                    let midpoint = ((self.positions[source].to_vec2()
                                        + self.positions[id].to_vec2())
                                        * 0.5)
                                        .to_pos2();
                                    self.positions.insert(edge_id, midpoint);
                                }
                            } else {
                                // Start edge creation mode.
                                self.edge_mode = Some(id);
                            }
                        }
                        // 2c) Normal left-click => either select or start dragging
                        else if ctx.input(|i| {
                            i.pointer.button_pressed(egui::PointerButton::Primary)
                        }) {
                            self.selected = Some(id);
                            // Attempt to drag if this is a "real" node (not an edge node).
                            if self.graph.get_edge(id).is_none() {
                                // Start dragging
                                self.dragging = Some(id);
                                // We could store an offset if we want more precise
                                // "grab from center" dragging, but it's optional.
                                self.drag_offset = None;
                            }
                        }
                    } else {
                        // 2d) Click on empty space => Add a new node
                        if ctx.input(|i| {
                            i.pointer.button_pressed(egui::PointerButton::Primary)
                                && !i.modifiers.shift
                        }) {
                            let new_id = self.graph.add_node(NodeData::default());
                            self.positions.insert(new_id, local_pos);
                        }
                    }
                }
            }

            // -----------------------------------------------------------
            // 3) Draw edges
            // -----------------------------------------------------------
            for edge in self.graph.edges_iter() {
                if let (Some(src), Some(tgt), Some(mid)) = (
                    self.positions.get(edge.source),
                    self.positions.get(edge.target),
                    self.positions.get(edge.id),
                ) {
                    painter.line_segment(
                        [to_screen(*src), to_screen(*mid)],
                        Stroke::new(1.5, Color32::LIGHT_BLUE),
                    );
                    painter.line_segment(
                        [to_screen(*mid), to_screen(*tgt)],
                        Stroke::new(1.5, Color32::LIGHT_BLUE),
                    );
                }
            }

            // -----------------------------------------------------------
            // 4) Draw nodes (and edge-nodes)
            // -----------------------------------------------------------
            for (id, pos) in &self.positions {
                let is_edge = self.graph.get_edge(id).is_some();
                let color = if !is_edge {
                    Color32::LIGHT_GREEN
                } else {
                    Color32::LIGHT_BLUE // edge node
                };
                let rect = Rect::from_center_size(to_screen(*pos), egui::vec2(20.0, 20.0));
                painter.rect(
                    rect,
                    5.0,
                    color,
                    Stroke::new(1.0, Color32::BLACK),
                    StrokeKind::Middle,
                );
                // (Optional) Debug: label with ID
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:?}", id.data().as_ffi()),
                    egui::TextStyle::Body.resolve(&ctx.style()),
                    Color32::BLACK,
                );
            }

            // -----------------------------------------------------------
            // 5) Visual feedback for edge creation
            // -----------------------------------------------------------
            if let Some(edge_src) = self.edge_mode {
                if let Some(pos) = self.positions.get(edge_src) {
                    let highlight_rect =
                        Rect::from_center_size(to_screen(*pos), egui::vec2(26.0, 26.0));
                    painter.rect(
                        highlight_rect,
                        8.0,
                        Color32::TRANSPARENT,
                        Stroke::new(2.0, Color32::RED),
                        StrokeKind::Middle,
                    );
                }
            }

            // -----------------------------------------------------------
            // 6) Cleanup the cached positions
            // -----------------------------------------------------------
            self.cleanup_positions();
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Graph Editor",
        options,
        Box::new(|_cc| Ok(Box::new(GraphEditor::default()))),
    )
}
