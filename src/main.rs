// main.rs
use node_simulator::editor::GraphEditor;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Graph Editor with Camera Controls",
        options,
        Box::new(|_cc| Ok(Box::new(GraphEditor::default()))),
    )
}