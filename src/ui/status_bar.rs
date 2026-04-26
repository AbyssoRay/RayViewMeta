use crate::app::RayviewApp;
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(34.0)
        .frame(
            egui::Frame::none()
                .fill(theme::HEADER)
                .inner_margin(egui::Margin::symmetric(14.0, 5.0)),
        )
        .show(ctx, |ui| {
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
