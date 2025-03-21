mod graph;

use eframe::{egui, App, Frame};
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, Key};
use graph::{Graph, ID, NodeData};
use slotmap::{Key as SlotKey, SecondaryMap};
use std::collections::{HashSet, VecDeque};

use rfd::FileDialog;
use std::fs::File;
use std::io::{Read, Write};

// A wrapper to serialize egui::Pos2.
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializablePos2 {
    x: f32,
    y: f32,
}

impl From<egui::Pos2> for SerializablePos2 {
    fn from(pos: egui::Pos2) -> Self {
        Self { x: pos.x, y: pos.y }
    }
}

impl From<SerializablePos2> for egui::Pos2 {
    fn from(s: SerializablePos2) -> Self {
        egui::pos2(s.x, s.y)
    }
}

// SaveData stores the graph and positions (not GUI state).
#[derive(serde::Serialize, serde::Deserialize)]
struct SaveData {
    graph: Graph,
    // Convert SecondaryMap<ID, Pos2> into a Vec of (ID, SerializablePos2) pairs.
    positions: Vec<(ID, SerializablePos2)>,
}

struct GraphEditor {
    graph: Graph,
    positions: SecondaryMap<ID, Pos2>,
    selected: Option<ID>,
    edge_mode: Option<ID>,
    // dragging is now a bool, always applying to the selected node.
    dragging: bool,
    drag_offset: Option<Vec2>,
}

impl Default for GraphEditor {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            positions: SecondaryMap::new(),
            selected: None,
            edge_mode: None,
            dragging: false,
            drag_offset: None,
        }
    }
}

impl GraphEditor {
    fn cleanup_positions(&mut self) {
        self.positions.retain(|id, _| {
            self.graph.get_node(id).is_some() || self.graph.get_edge(id).is_some()
        });
    }

    // Recursively update midpoints for edge-nodes connected to `start_id`.
    fn update_positions_recursive(&mut self, start_id: ID) {
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

    // Save the graph and positions using a native file dialog.
    fn save_graph(&self) -> std::io::Result<()> {
        if let Some(path) = FileDialog::new()
            .set_title("Save Graph")
            .set_file_name("graph_save.bin")
            .save_file()
        {
            let positions: Vec<(ID, SerializablePos2)> = self
                .positions
                .iter()
                .map(|(id, pos)| (id, pos.clone().into()))
                .collect();
            let save_data = SaveData {
                graph: self.graph.clone(),
                positions,
            };
            let encoded = bincode::serialize(&save_data)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            let mut file = File::create(path)?;
            file.write_all(&encoded)?;
        }
        Ok(())
    }

    // Load the graph and positions using a native file dialog.
    fn load_graph(&mut self) -> std::io::Result<()> {
        if let Some(path) = FileDialog::new().set_title("Load Graph").pick_file() {
            let mut file = File::open(path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            let save_data: SaveData = bincode::deserialize(&buffer)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            self.graph = save_data.graph;
            let mut new_positions: SecondaryMap<ID, Pos2> = SecondaryMap::new();
            for (id, spos) in save_data.positions {
                new_positions.insert(id, spos.into());
            }
            self.positions = new_positions;
            self.selected = None;
            self.edge_mode = None;
            self.dragging = false;
            self.drag_offset = None;
        }
        Ok(())
    }

    // Clear the current graph state (new blank slate).
    fn new_graph(&mut self) {
        self.graph = Graph::new();
        self.positions.clear();
        self.selected = None;
        self.edge_mode = None;
        self.dragging = false;
        self.drag_offset = None;
    }
}

impl App for GraphEditor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        // Keyboard shortcuts for saving, loading, and new state.
        if ctx.input(|i| i.key_pressed(Key::S) && i.modifiers.ctrl) {
            if let Err(e) = self.save_graph() {
                eprintln!("Error saving graph: {e}");
            }
        }
        if ctx.input(|i| i.key_pressed(Key::O) && i.modifiers.ctrl) {
            if let Err(e) = self.load_graph() {
                eprintln!("Error loading graph: {e}");
            }
        }
        if ctx.input(|i| i.key_pressed(Key::N) && i.modifiers.ctrl) {
            self.new_graph();
        }

        // Top panel with Save and Load buttons.
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("💾 Save").clicked() {
                    if let Err(e) = self.save_graph() {
                        eprintln!("Error saving graph: {e}");
                    }
                }
                if ui.button("📂 Load").clicked() {
                    if let Err(e) = self.load_graph() {
                        eprintln!("Error loading graph: {e}");
                    }
                }
                if ui.button("✚ New").clicked() {
                    self.new_graph();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Left-click empty space: add node");
            ui.label("Shift + click two nodes: connect with edge");
            ui.label("Right-click: delete node/edge");
            ui.label("Drag node: move with edge updates (cannot leave visible area)");
            ui.label("Ctrl+S: save, Ctrl+O: load, Ctrl+N: new");

            let (response, painter) =
                ui.allocate_painter(ui.available_size(), Sense::drag());
            let to_screen = |pos: Pos2| response.rect.left_top() + pos.to_vec2();

            // DRAG LOGIC: if dragging, update the position of the selected node.
            if self.dragging {
                if !ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                    self.dragging = false;
                    self.drag_offset = None;
                    if let Some(id) = self.selected {
                        self.update_positions_recursive(id);
                    }
                } else if let Some(id) = self.selected {
                    if let Some(old_pos) = self.positions.get(id) {
                        let delta = ctx.input(|i| i.pointer.delta());
                        if delta != Vec2::ZERO {
                            let mut new_pos = *old_pos + delta;
                            // Clamp new_pos so the node (20x20) remains within response.rect.
                            let half_size = 10.0;
                            new_pos.x = new_pos.x.clamp(half_size, response.rect.width() - half_size);
                            new_pos.y = new_pos.y.clamp(half_size, response.rect.height() - half_size);
                            self.positions.insert(id, new_pos);
                        }
                    }
                }
            }

            // Handle pointer input for clicks, edge creation, deletion, etc.
            if ctx.input(|i| i.pointer.any_pressed()) {
                if let Some(click_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    let local_pos = (click_pos - response.rect.left_top()).to_pos2();
                    // Find an element under the pointer.
                    let clicked_id = self
                        .positions
                        .iter()
                        .find(|(_, &pos)| pos.distance(local_pos) < 20.0)
                        .map(|(id, _)| id);
                    if let Some(id) = clicked_id {
                        // Right-click: delete the element.
                        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
                            if self.graph.get_node(id).is_some() {
                                self.graph.remove_node(id);
                            } else if self.graph.get_edge(id).is_some() {
                                self.graph.remove_edge(id);
                            }
                            self.positions.remove(id);
                        }
                        // Shift+Left-click: create an edge.
                        else if ctx.input(|i| {
                            i.modifiers.shift
                                && i.pointer.button_pressed(egui::PointerButton::Primary)
                        }) {
                            if let Some(source) = self.edge_mode.take() {
                                if let Some(edge_id) = self.graph.add_edge(source, id) {
                                    let mid = ((self.positions[source].to_vec2()
                                        + self.positions[id].to_vec2())
                                        * 0.5)
                                        .to_pos2();
                                    self.positions.insert(edge_id, mid);
                                }
                            } else {
                                self.edge_mode = Some(id);
                            }
                        }
                        // Left-click: select and, if it's a real node, start dragging.
                        else if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                        {
                            self.selected = Some(id);
                            if self.graph.get_edge(id).is_none() {
                                self.dragging = true;
                                self.drag_offset = None;
                            }
                        }
                    } else if ctx.input(|i| {
                        i.pointer.button_pressed(egui::PointerButton::Primary)
                            && !i.modifiers.shift
                    }) {
                        let new_id = self.graph.add_node(NodeData::default());
                        self.positions.insert(new_id, local_pos);
                    }
                }
            }

            // Draw edges.
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

            // Draw nodes and edge-nodes.
            for (id, pos) in &self.positions {
                let is_edge = self.graph.get_edge(id).is_some();
                let color = if !is_edge {
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
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:?}", id.data().as_ffi()),
                    egui::TextStyle::Body.resolve(&ctx.style()),
                    Color32::BLACK,
                );
            }

            // Highlight node in edge creation mode.
            if let Some(src) = self.edge_mode {
                if let Some(pos) = self.positions.get(src) {
                    let highlight = Rect::from_center_size(to_screen(*pos), egui::vec2(26.0, 26.0));
                    painter.rect(
                        highlight,
                        8.0,
                        Color32::TRANSPARENT,
                        Stroke::new(2.0, Color32::RED),
                        StrokeKind::Middle,
                    );
                }
            }

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
