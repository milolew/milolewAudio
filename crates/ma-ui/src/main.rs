//! milolew Audio — GUI entry point.

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("milolew Audio")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "milolew Audio",
        options,
        Box::new(|cc| Ok(Box::new(ma_ui::app::DawApp::new(cc)))),
    )
}
