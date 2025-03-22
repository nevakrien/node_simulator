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
    edge_mode: Option<ID>,
    dragging: bool,
    panning: bool,
    show_help: bool,

}

impl Default for GraphEditor {
    fn default() -> Self {
        Self {
            state: GraphState::default(),
            selected: None,
            edge_mode: None,
            dragging: false,
            panning: false,
            show_help: true,

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
        // If it's a node (not an edge), start dragging
        if self.state.graph.get_edge(id).is_none() {
            self.dragging = true;
        }
    }

    fn handle_edge_creation(&mut self, id: ID) {
        if let Some(src) = self.edge_mode.take() {
            self.state.add_edge_between(src, id);
        } else {
            self.edge_mode = Some(id);
        }
    }

    fn create_node(&mut self, pos: Pos2) {
        self.state.add_node_at(pos);
    }

    fn delete_element(&mut self, id: ID) {
        self.state.remove_element(id);
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
            self.edge_mode = None;
            self.dragging = false;
            self.panning = false;
        }
        Ok(())
    }

    fn new_graph(&mut self) {
        self.state = GraphState::default();
        self.selected = None;
        self.edge_mode = None;
        self.dragging = false;
        self.panning = false;
    }

    fn reset_camera(&mut self) {
        self.state.camera.reset();
    }

    // ─────────────────────────────────────────────────────────────
    //  2) Single Function for Reading ALL Raw Input
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

    // If the mouse is not inside the main drawing area, skip
    if !response.hovered() {
        return;
    }

    // ── ZOOM (mouse wheel) ─────────────────────────────────────
    let scroll_delta = input.raw_scroll_delta.y;
    if scroll_delta.abs() > 0.0 {
        if let Some(cursor_pos) = input.pointer.hover_pos() {
            self.zoom_camera(cursor_pos, screen_origin, scroll_delta);
        }
        // don't return; we can zoom + do other stuff in the same frame
    }

    // ── PAN (middle mouse or Alt+Left) ─────────────────────────
    let middle_down = input.pointer.button_down(PointerButton::Middle);
    let alt_left_down = input.modifiers.alt && input.pointer.button_down(PointerButton::Primary);
    if middle_down || alt_left_down {
        self.panning = true;
        let delta = input.pointer.delta();
        if delta != Vec2::ZERO {
            self.pan_camera(delta);
        }
        return;
    } else {
        self.panning = false;
    }

    // ── DRAGGING (already selected node) ───────────────────────
    if self.dragging && !self.panning {
        // If the button was released, stop dragging
        if !input.pointer.button_down(PointerButton::Primary) {
            self.dragging = false;
            return;
        }
        // Otherwise move the node with the pointer
        if let Some(id) = self.selected {
            if let Some(pointer_pos) = input.pointer.hover_pos() {
                let new_world_pos = self.to_world(pointer_pos, screen_origin);
                self.move_node(id, new_world_pos);
            }
        }
        return;
    }

    // ── LEFT-CLICK EMPTY SPACE => CREATE NODE ──────────────────
    // If you want “click empty space => create new node,” we do it here:
    if input.pointer.button_pressed(PointerButton::Primary)
        && !input.modifiers.alt
        && !input.modifiers.shift
    {
        // We only create a node if the user didn't click a node (that logic is handled elsewhere)
        // So we check if the pointer is in "empty space" (no node responded):
        // Easiest check is: does egui want pointer input? If so, some other widget used it
        // If not, we create a node here
        if !ctx.is_pointer_over_area() {
            if let Some(cursor_pos) = input.pointer.hover_pos() {
                let world_pos = self.to_world(cursor_pos, screen_origin);
                self.create_node(world_pos);
                return;
            }
        }
    }
}

fn process_node_input(&mut self, node_id: ID, response: &egui::Response, screen_origin: Pos2) {
    let input = response.ctx.input(|i| i.clone());

    // ── Right-click → delete ───────────────────────
    if response.clicked_by(PointerButton::Secondary) {
        self.delete_element(node_id);
        return;
    }

    // ── Shift + left-click → edge creation ─────────
    if response.clicked_by(PointerButton::Primary) && input.modifiers.shift {
        self.handle_edge_creation(node_id);
        return;
    }

    // ── Regular left-click → select ────────────────
    if response.clicked_by(PointerButton::Primary) && !input.modifiers.shift {
        self.select_element(node_id); // still needed for drag tracking
    }

    // ── Drag movement for selected node ────────────
    if response.dragged(){
        if let Some(pointer_pos) = input.pointer.hover_pos() {
            let new_world_pos = self.to_world(pointer_pos, screen_origin);
            self.move_node(node_id, new_world_pos);
        }
    }
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
                ui.label(format!("Zoom: {:.1}x", self.state.camera.zoom));

                // Right-justified help toggle
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let help_text = if self.show_help { "❓ Hide Help" } else { "❓ Show Help" };
                    if ui.button(help_text).clicked() {
                        // This button click is consumed by the UI
                        // so we won't also create a node from the same click
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

        // On older egui, you must pass an `Id` instead of a string
        let help_area = egui::Area::new(egui::Id::new("help_overlay"))
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 50.0))
            .movable(false)
            .interactable(false);

        help_area.show(ctx, |ui| {
            // Example of a dark background
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

fn draw_graph(
    &mut self,
    painter: &egui::Painter,
    screen_origin: Pos2,
    ctx: &egui::Context,
    ui: &mut egui::Ui
) {
    // Draw edges
    for edge in self.state.graph.edges_iter() {
        if let (Some(src), Some(tgt), Some(mid)) = (
            self.state.positions.get(edge.source),
            self.state.positions.get(edge.target),
            self.state.positions.get(edge.id),
        ) {
            painter.line_segment(
                [
                    self.to_screen(*src, screen_origin),
                    self.to_screen(*mid, screen_origin),
                ],
                Stroke::new(1.5, Color32::LIGHT_BLUE),
            );
            painter.line_segment(
                [
                    self.to_screen(*mid, screen_origin),
                    self.to_screen(*tgt, screen_origin),
                ],
                Stroke::new(1.5, Color32::LIGHT_BLUE),
            );
        }
    }

    // Allocate a UI layer exactly over the painter area
    ui.allocate_ui_at_rect(painter.clip_rect(), |ui| {
        for (id, pos) in self.state.positions.clone() {
            let is_edge = self.state.graph.get_edge(id).is_some();
            let color = if is_edge {
                Color32::LIGHT_BLUE
            } else {
                Color32::LIGHT_GREEN
            };

            // Where to draw the node in screen space
            let node_size = egui::vec2(20.0, 20.0) * self.state.camera.zoom;
            let screen_pos = self.to_screen(pos, screen_origin);
            let rect = Rect::from_center_size(screen_pos, node_size);
            let corner_radius = 5.0 * self.state.camera.zoom;

            // Draw the node
            painter.rect(rect, corner_radius, color, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);

            // Interactable region:
            let response = ui.interact(rect, ui.id().with(id), Sense::all());
            // Hand off the logic to our new function
            self.process_node_input(id, &response,screen_origin);

            // If you want more debug prints:
            // if response.hovered()  { println!("Node {id:?} hovered"); }
            // etc.

            // Show ID if zoomed in enough
            if self.state.camera.zoom > 0.4 {
                let text_style = if self.state.camera.zoom < 0.7 {
                    egui::TextStyle::Small
                } else {
                    egui::TextStyle::Body
                };
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:?}", id.data().as_ffi() as u32),
                    text_style.resolve(&ctx.style()),
                    Color32::BLACK,
                );
            }
        }
    });

    // Highlight any node in edge-creation mode
    if let Some(src) = self.edge_mode {
        if let Some(pos) = self.state.positions.get(src) {
            let highlight_size = egui::vec2(26.0, 26.0) * self.state.camera.zoom;
            let highlight_rect =
                Rect::from_center_size(self.to_screen(*pos, screen_origin), highlight_size);
            painter.rect(
                highlight_rect,
                8.0 * self.state.camera.zoom,
                Color32::TRANSPARENT,
                Stroke::new(2.0, Color32::RED),
                StrokeKind::Middle,
            );
        }
    }
}

    // ─────────────────────────────────────────────────────────────
    //  4) Coordinate Helpers
    // ─────────────────────────────────────────────────────────────

    fn to_screen(&self, pos: Pos2, screen_origin: Pos2) -> Pos2 {
        self.state.camera.world_to_screen(pos, screen_origin)
    }

    fn to_world(&self, pos: Pos2, screen_origin: Pos2) -> Pos2 {
        self.state.camera.screen_to_world(pos, screen_origin)
    }
}

// ─────────────────────────────────────────────────────────────────
//  5) eframe::App Implementation
// ─────────────────────────────────────────────────────────────────

impl App for GraphEditor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.draw_top_panel(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let screen_origin = response.rect.left_top();

            // All input is handled in one place:
            self.process_global_input(ctx, &response, screen_origin);

            // Cleanup orphaned positions, then draw
            self.state.cleanup_positions();
            self.draw_graph(&painter, screen_origin, ctx,ui);
        });

        self.draw_help_overlay(ctx);
    }
}
