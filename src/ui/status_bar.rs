use crate::app::RayviewApp;
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::Panel::bottom("status_bar")
        .exact_size(34.0)
        .frame(
            egui::Frame::new()
                .fill(theme::HEADER)
                .inner_margin(egui::Margin::symmetric(14, 5)),
        )
        .show_inside(root_ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Status").small().color(theme::CYAN));
                ui.label(egui::RichText::new(&app.status).small().color(theme::TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Endpoint {}", app.api.base_url()))
                            .monospace()
                            .small()
                            .color(theme::MUTED),
                    );
                });
            });
        });
}
