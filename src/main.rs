mod graph;

use eframe::{egui, App, Frame};
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind};
use graph::{Graph, ID, NodeData};
use slotmap::Key;
use std::collections::HashMap;

struct GraphEditor {
    graph: Graph,
    /// Stores on-screen positions for both nodes and edge-nodes.
    positions: HashMap<ID, Pos2>,
    /// Currently selected node (if any).
    selected: Option<ID>,
    /// If set, holds the source node for creating an edge.
    edge_mode: Option<ID>,
}

impl Default for GraphEditor {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            positions: HashMap::new(),
            selected: None,
            edge_mode: None,
        }
    }
}

impl GraphEditor {
    /// Clean up the positions cache by removing IDs that are no longer in the graph.
    fn cleanup_positions(&mut self) {
        self.positions.retain(|&id, _| {
            self.graph.get_node(id).is_some() || self.graph.get_edge(id).is_some()
        });
    }
}

impl App for GraphEditor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Left-click on empty space: create a node");
            ui.label("Shift + Left-click on a node: create an edge (click two nodes)");
            ui.label("Right-click on a node/edge: delete it");

            // Allocate a painter for drawing.
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), Sense::click_and_drag());

            // Helper: convert a local position to screen coordinates.
            let to_screen = |pos: Pos2| -> Pos2 {
                response.rect.left_top() + pos.to_vec2()
            };

            // --- Handle pointer input ---
            if ctx.input(|i| i.pointer.any_pressed()) {
                if let Some(click_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    let local_pos = (click_pos - response.rect.left_top()).to_pos2();
                    // Check if click is near an existing element.
                    let clicked_id = self.positions.iter().find(|(_, &pos)| {
                        pos.distance(local_pos) < 20.0
                    }).map(|(&id, _)| id);
                    
                    if let Some(id) = clicked_id {
                        // Right-click: delete the element.
                        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
                            if self.graph.get_node(id).is_some() {
                                self.graph.remove_node(id);
                            } else if self.graph.get_edge(id).is_some() {
                                self.graph.remove_edge(id);
                            }
                            self.positions.remove(&id);
                        }
                        // Shift+Left-click: create an edge.
                        else if ctx.input(|i| {
                            i.modifiers.shift &&
                            i.pointer.button_pressed(egui::PointerButton::Primary)
                        }) {
                            if let Some(source) = self.edge_mode.take() {
                                if let Some(edge_id) = self.graph.add_edge(source, id) {
                                    // Compute the midpoint for the edge-node.
                                    let midpoint = ((self.positions[&source].to_vec2() +
                                        self.positions[&id].to_vec2()) * 0.5)
                                        .to_pos2();
                                    self.positions.insert(edge_id, midpoint);
                                }
                            } else {
                                // Start edge creation mode.
                                self.edge_mode = Some(id);
                            }
                        }
                        // Left-click (without shift): select the element.
                        else if ctx.input(|i| {
                            i.pointer.button_pressed(egui::PointerButton::Primary)
                        }) {
                            self.selected = Some(id);
                        }
                    } else {
                        // Click on empty space: add a new node.
                        if ctx.input(|i| {
                            i.pointer.button_pressed(egui::PointerButton::Primary)
                        }) {
                            let new_id = self.graph.add_node(NodeData::default());
                            self.positions.insert(new_id, local_pos);
                        }
                    }
                }
            }

            // --- Draw edges ---
            for edge in self.graph.edges_iter() {
                if let (Some(src), Some(tgt), Some(mid)) = (
                    self.positions.get(&edge.source),
                    self.positions.get(&edge.target),
                    self.positions.get(&edge.id),
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

            // --- Draw nodes and edge-nodes ---
            for (id, pos) in &self.positions {
                let is_node = self.graph.get_node(*id).is_some();
                let color = if is_node {
                    Color32::LIGHT_GREEN
                } else {
                    Color32::LIGHT_BLUE
                };
                let rect = Rect::from_center_size(to_screen(*pos), egui::vec2(20.0, 20.0));
                painter.rect(
                    rect,
                    5.0,
                    color,
                    Stroke::new(1.0, Color32::BLACK),
                    StrokeKind::Middle,
                );
                // (Optional) Draw the node's ID for debugging.
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:?}", id.data().as_ffi()),
                    egui::TextStyle::Body.resolve(&ctx.style()),
                    Color32::BLACK,
                );
            }

            // --- Visual feedback for edge creation ---
            if let Some(edge_src) = self.edge_mode {
                if let Some(pos) = self.positions.get(&edge_src) {
                    let highlight_rect = Rect::from_center_size(to_screen(*pos), egui::vec2(26.0, 26.0));
                    painter.rect(
                        highlight_rect,
                        8.0,
                        Color32::TRANSPARENT,
                        Stroke::new(2.0, Color32::RED),
                        StrokeKind::Middle,
                    );
                }
            }

            // --- Cleanup the cached positions ---
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
