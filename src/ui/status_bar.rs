use crate::app::RayviewApp;
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::Panel::bottom("status_bar")
        .exact_size(42.0)
        .frame(
            egui::Frame::new()
                .fill(theme::HEADER)
                .inner_margin(egui::Margin::symmetric(14, 7)),
        )
        .show_inside(root_ui, |ui| {
            ui.horizontal(|ui| {
                let busy = app.loading || !app.translation_inflight.is_empty();
                ui.label(egui::RichText::new("Status").small().color(theme::ACCENT));
                if let Some(progress) = &app.import_progress {
                    let fraction = if progress.total == 0 {
                        0.0
                    } else {
                        (progress.current as f32 / progress.total as f32).clamp(0.0, 1.0)
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(150.0)
                            .text(format!("({}/{})", progress.current, progress.total)),
                    )
                    .on_hover_text(&progress.item);
                }
                ui.label(egui::RichText::new(&app.status).small().color(theme::TEXT));
                if busy {
                    ui.spinner();
                }
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
