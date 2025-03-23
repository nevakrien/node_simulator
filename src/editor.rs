use egui::UiBuilder;
use slotmap::Key as SlotKey;
use eframe::{egui, App, Frame};
use egui::{
    Color32, Key, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};
use rfd::FileDialog;
use std::io;

use crate::state::GraphState;
use crate::graph::ID;

pub struct GraphEditor {
    pub state: GraphState,
    selected: Option<ID>,
    // edge_mode: Option<ID>,
    show_help: bool,
    highlight: bool,
}

impl Default for GraphEditor {
    fn default() -> Self {
        Self {
            state: GraphState::default(),
            selected: None,
            // edge_mode: None,
            show_help: false,
            highlight:true,
        }
    }
}

impl GraphEditor {
    // ─────────────────────────────────────────────────────────────
    //  1) Graph Operations (pure state changes)
    // ─────────────────────────────────────────────────────────────

    fn zoom_camera(&mut self, cursor_pos: Pos2, screen_origin: Pos2, scroll_delta: f32) {
        if scroll_delta.abs() < f32::EPSILON {
            return;
        }
        let fixed_point = self.state.camera.screen_to_world(cursor_pos, screen_origin);
        let zoom_factor = if scroll_delta > 0.0 { 1.1 } else { 1.0 / 1.1 };
        self.state.camera.zoom = (self.state.camera.zoom * zoom_factor).clamp(0.1, 5.0);

        let new_screen_pos = screen_origin
            + (fixed_point.to_vec2() + self.state.camera.offset) * self.state.camera.zoom;
        let adjustment = (cursor_pos - new_screen_pos) / self.state.camera.zoom;
        self.state.camera.offset += adjustment;
    }

    fn pan_camera(&mut self, delta: Vec2) {
        self.state.camera.offset += delta / self.state.camera.zoom;
    }

    fn move_node(&mut self, id: ID, new_pos: Pos2) {
        self.state.positions.insert(id, new_pos);
        self.state.update_positions_recursive(id);
    }

    fn select_element(&mut self, id: ID) {
        self.selected = Some(id);
        // We no longer set "dragging = true" here because we handle dragging in process_node_input
    }

    fn handle_edge_creation(&mut self, id: ID) {
        if let Some(src) = self.selected {
            self.state.add_edge_between(src, id);
        } else {
            self.selected = Some(id);
        }
    }

    fn create_node(&mut self, pos: Pos2) {
        self.selected = Some(
            self.state.add_node_at(pos));
    }

    fn delete_element(&mut self, id: ID) {
        self.state.remove_element(id);

        //not needed for now but keep it in for good measure
        if self.selected == Some(id) {
            self.selected = None;
        }
    }

    fn save_graph(&self) -> io::Result<()> {
        if let Some(path) = FileDialog::new()
            .set_title("Save Graph")
            .set_file_name("graph_save.bin")
            .save_file()
        {
            self.state.save_to_file(&path)?;
        }
        Ok(())
    }

    fn load_graph(&mut self) -> io::Result<()> {
        if let Some(path) = FileDialog::new().set_title("Load Graph").pick_file() {
            self.state = GraphState::load_from_file(&path)?;
            // Reset
            self.selected = None;
        }
        Ok(())
    }

    fn new_graph(&mut self) {
        self.state = GraphState::default();
        self.selected = None;
    }

    fn reset_camera(&mut self) {
        self.state.camera.reset();
    }

    fn toggle_highlight(&mut self) {
        self.highlight = !self.highlight;
    }


    // ─────────────────────────────────────────────────────────────
    //  2) Input Handling (drag, shift-click, etc.)
    // ─────────────────────────────────────────────────────────────
    #[allow(clippy::needless_return)]
    fn process_global_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        screen_origin: Pos2,
    ) {
        let input = ctx.input(|i| i.clone());

        // ── KEYBOARD SHORTCUTS ────────────────────────────────────
        if input.key_pressed(Key::S) && input.modifiers.ctrl {
            let _ = self.save_graph();
            return;
        } else if input.key_pressed(Key::O) && input.modifiers.ctrl {
            let _ = self.load_graph();
            return;
        } else if input.key_pressed(Key::N) && input.modifiers.ctrl {
            self.new_graph();
            return;
        } else if input.key_pressed(Key::Home) {
            self.reset_camera();
            return;
        }

        //__ Deselect on any delete like op  ─────────────────────────
        if input.pointer.button_pressed(PointerButton::Secondary){
            self.selected = None;

        }

        // If the mouse is not inside the main drawing area, skip
        if !response.hovered() {
            return;
        }



        // ── ZOOM (mouse wheel) ────────────────────────────────────
        let scroll_delta = input.raw_scroll_delta.y;
        if scroll_delta.abs() > 0.0 {
            if let Some(cursor_pos) = input.pointer.hover_pos() {
                self.zoom_camera(cursor_pos, screen_origin, scroll_delta);
            }
            // no return, we can zoom + do other stuff in the same frame
        }

        // ── PAN (middle mouse or Alt+Left) ────────────────────────
        let middle_down = input.pointer.button_down(PointerButton::Middle);
        let alt_left_down = input.modifiers.alt && input.pointer.button_down(PointerButton::Primary);
        if middle_down || alt_left_down {
            let delta = input.pointer.delta();
            if delta != Vec2::ZERO {
                self.pan_camera(delta);
            }
            return;
        }
        // ── LEFT-CLICK EMPTY SPACE => CREATE NODE ──────────────────
        if input.pointer.button_pressed(PointerButton::Primary)
            && !input.modifiers.alt
            && !input.modifiers.shift
        {
            // If no other widget used the click, we create a node
            if !ctx.is_pointer_over_area() {
                if let Some(cursor_pos) = input.pointer.hover_pos() {
                    let world_pos = self.to_world(cursor_pos, screen_origin);
                    self.create_node(world_pos);
                    return;
                }
            }

            return;
        }

        
    }

    fn process_node_input(&mut self, node_id: ID, response: &egui::Response, screen_origin: Pos2) {
        let input = response.ctx.input(|i| i.clone());

        // Right-click => delete node
        if response.clicked_by(PointerButton::Secondary) {
            self.delete_element(node_id);
            return;
        }

        // Shift+left-click => edge creation
        if response.clicked_by(PointerButton::Primary) && input.modifiers.shift {
            self.handle_edge_creation(node_id);
            return;
        }

        // Regular left-click => select node
        if response.clicked_by(PointerButton::Primary) && !input.modifiers.shift {
            self.select_element(node_id);
        }

        // Drag movement for selected node
        // We'll only move if it's the selected node and the user is dragging
        // This means you can drag multiple nodes if you click them in the same frame,
        // but typically you won't. This is one approach. 
        if response.dragged() {
            // self.selected = Some(node_id);
            // self.selected = None;

            if let Some(pointer_pos) = input.pointer.hover_pos() {
                let new_world_pos = self.to_world(pointer_pos, screen_origin);
                self.move_node(node_id, new_world_pos);
            }
        }
    }

    fn process_edge_segment_input(&mut self, edge_id: ID, response: &egui::Response) {
        let input = response.ctx.input(|i| i.clone());

        if response.clicked_by(PointerButton::Secondary) {
            self.delete_element(edge_id);
        }

        if response.clicked_by(PointerButton::Primary) && input.modifiers.shift {
            self.handle_edge_creation(edge_id);
        }

        // You could add selection or dragging here if desired
    }



    // ─────────────────────────────────────────────────────────────
    //  3) UI & Drawing
    // ─────────────────────────────────────────────────────────────

    fn draw_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("💾 Save").clicked() {
                    let _ = self.save_graph();
                }
                if ui.button("📂 Load").clicked() {
                    let _ = self.load_graph();
                }
                if ui.button("✚ New").clicked() {
                    self.new_graph();
                }
                if ui.button("🏠 Reset Camera").clicked() {
                    self.reset_camera();
                }

                if ui.button("🎯 Toggle Highlight").clicked() {
                    self.toggle_highlight();
                }

                ui.label(format!("Zoom: {:.1}x", self.state.camera.zoom));

                // Right-justified help toggle
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let help_text = if self.show_help { "❓ Hide Help" } else { "❓ Show Help" };
                    if ui.button(help_text).clicked() {
                        self.show_help = !self.show_help;
                    }
                });
            });
        });
    }

    fn draw_help_overlay(&self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }

        let help_area = egui::Area::new(egui::Id::new("help_overlay"))
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 50.0))
            .movable(false)
            .interactable(false);

        help_area.show(ctx, |ui| {
            ui.visuals_mut().widgets.noninteractive.bg_fill =
                Color32::from_rgba_premultiplied(0, 0, 0, 180);

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

fn draw_edge_segment(
    &mut self,
    edge_id: ID,
    start: Pos2,
    end: Pos2,
    ui: &mut egui::Ui,
    extra: &'static str,
) {
    let thickness = 11.0 * self.state.camera.zoom;
    let id = ui.id().with("edge").with(edge_id).with(extra);
    let rect = Rect::from_two_pos(start, end).expand(thickness);
    let response = ui.interact(rect, id, Sense::click());

    let pointer = ui.input(|i| i.pointer.clone());
    let mut hovered = false;

    if let Some(pos) = pointer.hover_pos() {
        if distance_to_segment(pos, start, end) <= thickness {
            hovered = true;
        }
    }

    if hovered && pointer.any_click() {
        self.process_edge_segment_input(edge_id, &response);
        ui.memory_mut(|m| m.request_focus(id));
    }

    let stroke = if self.highlight && hovered {
        Stroke::new(2.0, Color32::YELLOW)
    } else {
        Stroke::new(1.5, Color32::LIGHT_BLUE)
    };

    ui.painter().line_segment([start, end], stroke);
}


fn draw_graph(
    &mut self,
    painter: &egui::Painter,
    screen_origin: Pos2,
    ctx: &egui::Context,
    ui: &mut egui::Ui,
) {
    // Allocate one UI element for the entire drawing area
    ui.allocate_new_ui(UiBuilder::new().max_rect(painter.clip_rect()), |ui| {
        // 1) Draw edges
        let edges: Vec<_> = self.state.graph.edges_iter().cloned().collect();
        for edge in edges {
            if let (Some(src), Some(tgt), Some(mid)) = (
                self.state.positions.get(edge.source),
                self.state.positions.get(edge.target),
                self.state.positions.get(edge.id),
            ) {
                let screen_src = self.to_screen(*src, screen_origin);
                let screen_mid = self.to_screen(*mid, screen_origin);
                let screen_tgt = self.to_screen(*tgt, screen_origin);

                // Draw each segment of the edge with interactive hitboxes
                for (start, end, seg_label) in [
                    (screen_src, screen_mid, "src"),
                    (screen_mid, screen_tgt, "tgt"),
                ] {
                    self.draw_edge_segment(edge.id, start, end, ui, seg_label);
                }
            }
        }

        // 2) Draw all nodes (including edge nodes)
        for (id, pos) in self.state.positions.clone() {
            let is_edge = self.state.graph.get_edge(id).is_some();
            let base_color = if is_edge {
                Color32::LIGHT_BLUE
            } else {
                Color32::LIGHT_GREEN
            };

            let node_size = egui::vec2(20.0, 20.0) * self.state.camera.zoom;
            let screen_pos = self.to_screen(pos, screen_origin);
            let rect = Rect::from_center_size(screen_pos, node_size);
            let corner_radius = 5.0 * self.state.camera.zoom;

            // Interactable region
            let response = ui.interact(rect, ui.id().with("node").with(id), Sense::all());

            // Only turn fill yellow if self.highlight is true AND hovered
            let fill_color = if self.highlight && response.hovered() {
                Color32::YELLOW
            } else {
                base_color
            };

            // Always use a consistent black border
            let stroke = Stroke::new(1.0, Color32::BLACK);
            ui.painter().rect(rect, corner_radius, fill_color, stroke, StrokeKind::Middle);

            // Process node input (drag, delete, etc.)
            self.process_node_input(id, &response, screen_origin);

            // Optionally show text if zoomed in enough
            if self.state.camera.zoom > 0.4 {
                let text_style = if self.state.camera.zoom < 0.7 {
                    egui::TextStyle::Small
                } else {
                    egui::TextStyle::Body
                };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:?}", id.data().as_ffi() as u32),
                    text_style.resolve(&ctx.style()),
                    Color32::BLACK,
                );
            }
        }

        // 3) Draw a red highlight for the selected node, only if self.highlight is true
        if let Some(selected_id) = self.selected {
            if let Some(pos) = self.state.positions.get(selected_id) {
                let screen_pos = self.to_screen(*pos, screen_origin) + self.state.camera.zoom*Vec2{x:0.3,y:0.1};
                let node_radius = 10.0 * self.state.camera.zoom; // node is 20x20
                let highlight_radius = node_radius + 5.0 * self.state.camera.zoom;

                ui.painter().circle_stroke(
                    screen_pos,
                    highlight_radius,
                    Stroke::new(2.3, Color32::RED),
                );
            }
        }
    });
}


    fn to_screen(&self, pos: Pos2, screen_origin: Pos2) -> Pos2 {
        self.state.camera.world_to_screen(pos, screen_origin)
    }

    fn to_world(&self, pos: Pos2, screen_origin: Pos2) -> Pos2 {
        self.state.camera.screen_to_world(pos, screen_origin)
    }
}

// ─────────────────────────────────────────────────────────────────
//  4) eframe::App Implementation
// ─────────────────────────────────────────────────────────────────

impl App for GraphEditor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.draw_top_panel(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            // We use Sense::click_and_drag() here so we can do e.g. "drag from empty space"
            // if you ever want that. But it's mostly for capturing pointer input.
            let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let screen_origin = response.rect.left_top();

            // 1) Global input (zoom, pan, new node in empty space):
            self.process_global_input(ctx, &response, screen_origin);

            // 2) Cleanup orphaned positions, then draw the graph
            self.state.cleanup_positions();
            self.draw_graph(&painter, screen_origin, ctx, ui);
        });

        self.draw_help_overlay(ctx);
    }
}
fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let t = ap.dot(ab) / ab.length_sq();
    let t_clamped = t.clamp(0.0, 1.0);
    let closest = a + t_clamped * ab;
    p.distance(closest)
}
