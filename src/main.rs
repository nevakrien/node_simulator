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

// A wrapper to serialize egui::Vec2
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableVec2 {
    x: f32,
    y: f32,
}

impl From<egui::Vec2> for SerializableVec2 {
    fn from(vec: egui::Vec2) -> Self {
        Self { x: vec.x, y: vec.y }
    }
}

impl From<SerializableVec2> for egui::Vec2 {
    fn from(s: SerializableVec2) -> Self {
        egui::vec2(s.x, s.y)
    }
}

// SaveData stores the graph and positions (not GUI state).
#[derive(serde::Serialize, serde::Deserialize)]
struct SaveData {
    graph: Graph,
    // Convert SecondaryMap<ID, Pos2> into a Vec of (ID, SerializablePos2) pairs.
    positions: Vec<(ID, SerializablePos2)>,
    // Store camera settings
    camera_offset: SerializableVec2,
    camera_zoom: f32,
}

// Camera state to manage pan and zoom
struct Camera {
    offset: Vec2,     // Translation/pan offset
    zoom: f32,        // Zoom factor
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
    fn world_to_screen(&self, world_pos: Pos2, screen_origin: Pos2) -> Pos2 {
        screen_origin + ((world_pos.to_vec2() + self.offset) * self.zoom)
    }
    
    // Convert screen coordinates to world coordinates
    fn screen_to_world(&self, screen_pos: Pos2, screen_origin: Pos2) -> Pos2 {
        let screen_vec = screen_pos - screen_origin;
        (screen_vec / self.zoom - self.offset).to_pos2()
    }
}

struct GraphEditor {
    graph: Graph,
    positions: SecondaryMap<ID, Pos2>,
    selected: Option<ID>,
    edge_mode: Option<ID>,
    // dragging is now a bool, always applying to the selected node.
    dragging: bool,
    drag_offset: Option<Vec2>,
    // Camera for pan and zoom
    camera: Camera,
    // Track if we're currently panning the camera
    panning: bool,
    // Toggle to show/hide help text
    show_help: bool,
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
            camera: Camera::default(),
            panning: false,
            show_help: true, // Help is visible by default
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
                camera_offset: self.camera.offset.into(),
                camera_zoom: self.camera.zoom,
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
            
            // Load graph data
            self.graph = save_data.graph;
            
            // Load positions
            let mut new_positions: SecondaryMap<ID, Pos2> = SecondaryMap::new();
            for (id, spos) in save_data.positions {
                new_positions.insert(id, spos.into());
            }
            self.positions = new_positions;
            
            // Load camera settings
            self.camera.offset = save_data.camera_offset.into();
            self.camera.zoom = save_data.camera_zoom;
            
            // Reset interaction state
            self.selected = None;
            self.edge_mode = None;
            self.dragging = false;
            self.drag_offset = None;
            self.panning = false;
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
        // Reset camera to default
        self.camera = Camera::default();
        self.panning = false;
    }
    
    // Reset the camera view (center and 1.0 zoom)
    fn reset_camera(&mut self) {
        self.camera = Camera::default();
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
        // Reset camera with Home key
        if ctx.input(|i| i.key_pressed(Key::Home)) {
            self.reset_camera();
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
                if ui.button("🏠 Reset Camera").clicked() {
                    self.reset_camera();
                }
                // Display current zoom level
                ui.label(format!("Zoom: {:.1}x", self.camera.zoom));
                
                // Add spacer to push help button to the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let help_text = if self.show_help { "❓ Hide Help" } else { "❓ Show Help" };
                    if ui.button(help_text).clicked() {
                        self.show_help = !self.show_help;
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {

            let (response, painter) =
                ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let screen_origin = response.rect.left_top();
            
            // CAMERA CONTROLS - Handle zooming with mouse wheel
            if let Some(cursor_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                if response.rect.contains(cursor_pos) {
                    let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
                    if scroll_delta != 0.0 {
                        // Get the point in world coordinates that's currently under the cursor
                        let fixed_point = self.camera.screen_to_world(cursor_pos, screen_origin);
                        
                        // Update zoom level
                        let zoom_factor = if scroll_delta > 0.0 { 1.1 } else { 1.0 / 1.1 };
                        self.camera.zoom = (self.camera.zoom * zoom_factor).clamp(0.1, 5.0);
                        
                        // Calculate where fixed_point would appear on screen with the new zoom
                        let new_screen_pos = screen_origin + (fixed_point.to_vec2() + self.camera.offset) * self.camera.zoom;
                        
                        // Calculate needed adjustment to keep fixed_point under the cursor
                        let adjustment = (cursor_pos - new_screen_pos) / self.camera.zoom;
                        
                        // Apply the adjustment
                        self.camera.offset += adjustment;
                    }
                }
            }
            
            // CAMERA CONTROLS - Handle panning with middle mouse or Alt+Left drag
            let middle_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
            let alt_left_down = ctx.input(|i| {
                i.pointer.button_down(egui::PointerButton::Primary) && i.modifiers.alt
            });
            
            if (middle_down || alt_left_down) && !self.dragging {
                // Start panning
                if !self.panning {
                    self.panning = true;
                }
                // Apply pan
                let delta = ctx.input(|i| i.pointer.delta()) / self.camera.zoom;
                if delta != Vec2::ZERO {
                    self.camera.offset += delta;
                }
            } else {
                self.panning = false;
            }
            
            // Define helper function to convert between world and screen coords
            // let to_screen = |pos: Pos2| self.camera.world_to_screen(pos, screen_origin);
            let to_world = |pos: Pos2| self.camera.screen_to_world(pos, screen_origin);

            // DRAG LOGIC: if dragging, update the position of the selected node.
            if self.dragging && !self.panning {
                if !ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                    self.dragging = false;
                    self.drag_offset = None;
                    if let Some(id) = self.selected {
                        self.update_positions_recursive(id);
                    }
                } else if let Some(id) = self.selected {
                    if let Some(_old_pos) = self.positions.get(id) {
                        if let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                            // Convert pointer position to world space
                            let new_world_pos = to_world(pointer_pos);
                            
                            // No need to clamp in world space as we have camera panning
                            self.positions.insert(id, new_world_pos);
                        }
                    }
                }
            }

            //make borrow checker happy
            let to_screen = |pos: Pos2| self.camera.world_to_screen(pos, screen_origin);
            let to_world = |pos: Pos2| self.camera.screen_to_world(pos, screen_origin);

            // Handle pointer input for clicks, edge creation, deletion, etc.
            if ctx.input(|i| i.pointer.any_pressed()) && !self.panning {
                if let Some(click_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    let local_pos = to_world(click_pos);
                    
                    // Find an element under the pointer in world space
                    let clicked_id = self
                        .positions
                        .iter()
                        .find(|(_, &pos)| {
                            // Adjust hit test radius based on zoom
                            let hit_radius = 20.0 / self.camera.zoom;
                            pos.distance(local_pos) < hit_radius
                        })
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
                        else if ctx.input(|i| {
                            i.pointer.button_pressed(egui::PointerButton::Primary) 
                             && !i.modifiers.alt // Don't select when using Alt+drag for panning
                        }) {
                            self.selected = Some(id);
                            if self.graph.get_edge(id).is_none() {
                                self.dragging = true;
                                self.drag_offset = None;
                            }
                        }
                    } else if ctx.input(|i| {
                        i.pointer.button_pressed(egui::PointerButton::Primary)
                            && !i.modifiers.shift
                            && !i.modifiers.alt // Don't create node when using Alt+drag for panning
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
                
                // Scale the node size based on zoom
                let node_size = egui::vec2(20.0, 20.0) * self.camera.zoom;
                let rect = Rect::from_center_size(to_screen(*pos), node_size);
                let corner_radius = 5.0 * self.camera.zoom;
                
                painter.rect(
                    rect,
                    corner_radius,
                    color,
                    Stroke::new(1.0, Color32::BLACK),
                    StrokeKind::Middle,
                );
                
                // Only show ID text if zoom is sufficient for readability
                if self.camera.zoom > 0.4 {
                    let text_style = if self.camera.zoom < 0.7 {
                        egui::TextStyle::Small
                    } else {
                        egui::TextStyle::Body
                    };
                    
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{:?}", id.data().as_ffi()),
                        text_style.resolve(&ctx.style()),
                        Color32::BLACK,
                    );
                }
            }

            // Highlight node in edge creation mode.
            if let Some(src) = self.edge_mode {
                if let Some(pos) = self.positions.get(src) {
                    let highlight_size = egui::vec2(26.0, 26.0) * self.camera.zoom;
                    let highlight = Rect::from_center_size(to_screen(*pos), highlight_size);
                    painter.rect(
                        highlight,
                        8.0 * self.camera.zoom,
                        Color32::TRANSPARENT,
                        Stroke::new(2.0, Color32::RED),
                        StrokeKind::Middle,
                    );
                }
            }

            self.cleanup_positions();
        });
        
        // Draw help text as an overlay if enabled
        if self.show_help {
            // Create a semi-transparent overlay in the top-left corner
            let help_area = egui::Area::new("help_overlay".into())
                .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 50.0))
                .movable(false)
                .interactable(false);
                
            help_area.show(ctx, |ui| {
                ui.visuals_mut().widgets.noninteractive.bg_fill = Color32::from_rgba_premultiplied(0, 0, 0, 180);
                egui::Frame::NONE
                    .fill(Color32::from_rgba_premultiplied(0, 0, 0, 180))
                    .corner_radius(5.0)
                    .stroke(Stroke::new(1.0, Color32::from_gray(100)))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.set_max_width(300.0);
                        ui.label("Left-click empty space: add node");
                        ui.label("Shift + click two nodes: connect with edge");
                        ui.label("Right-click: delete node/edge");
                        ui.label("Drag node: move with edge updates");
                        ui.label("Middle-click drag or Alt+Left drag: pan view");
                        ui.label("Mouse wheel: zoom in/out");
                        ui.label("Home key or Reset Camera button: reset view");
                        ui.label("Ctrl+S: save, Ctrl+O: load, Ctrl+N: new");
                        ui.label("❓ button: toggle this help overlay");
                    });
            });
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Graph Editor with Camera Controls",
        options,
        Box::new(|_cc| Ok(Box::new(GraphEditor::default()))),
    )
}