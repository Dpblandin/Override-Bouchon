#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bouchonneur::app::BouchonneurApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_100.0, 700.0])
            .with_min_inner_size([760.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Bouchonneur - Déployeur de bouchons",
        options,
        Box::new(|creation_context| Ok(Box::new(BouchonneurApp::new(creation_context)))),
    )
}
