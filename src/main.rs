#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod api;
mod app;
mod assets;
mod doi;
mod pdf;
mod pubmed;
mod single_instance;
mod tasks;
mod translation;
mod ui;

use app::RayviewApp;

fn main() -> eframe::Result<()> {
    let Some(_single_instance) = single_instance::acquire_or_activate() else {
        return Ok(());
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 780.0])
        .with_min_inner_size([900.0, 600.0])
        .with_title("Rayview Meta");
    if let Some(icon) = assets::app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Rayview Meta",
        native_options,
        Box::new(|cc| Ok(Box::new(RayviewApp::new(cc)))),
    )
}
