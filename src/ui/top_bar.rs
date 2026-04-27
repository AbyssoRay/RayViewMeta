use crate::app::{RayviewApp, View};
use crate::ui::theme;

const HEADER_HEIGHT: f32 = 76.0;
const BUTTON_HEIGHT: f32 = 36.0;
const LOGO_WIDTH: f32 = 184.0;
const LOGO_HEIGHT: f32 = 48.0;
const NAV_BUTTON_COUNT: f32 = 5.0;
const NAV_BUTTON_GAP: f32 = 10.0;
const NAV_BUTTON_MIN_WIDTH: f32 = 82.0;
const NAV_BUTTON_MAX_WIDTH: f32 = 116.0;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::Panel::top("top_bar")
        .exact_size(HEADER_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(theme::HEADER)
                .inner_margin(egui::Margin::symmetric(16, 0)),
        )
        .show_inside(root_ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal_centered(|ui| {
                if let Some(texture) = &app.logo_texture {
                    ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(egui::vec2(LOGO_WIDTH, LOGO_HEIGHT))
                            .maintain_aspect_ratio(true),
                    );
                    ui.add_space(14.0);
                }
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Rayview Meta")
                            .strong()
                            .size(24.0)
                            .color(theme::TEXT),
                    );
                    ui.label(
                        egui::RichText::new(app.current_project_name())
                            .small()
                            .color(theme::MUTED),
                    );
                });
                ui.add_space(18.0);

                render_toolbar_buttons(ui, app);
                if app.loading {
                    ui.add_space(8.0);
                    ui.spinner();
                }
            });
            ui.add_space(8.0);
            ui.painter().line_segment(
                [ui.min_rect().left_bottom(), ui.min_rect().right_bottom()],
                egui::Stroke::new(1.0, theme::LINE_SOFT),
            );
        });
}

fn render_toolbar_buttons(ui: &mut egui::Ui, app: &mut RayviewApp) {
    ui.spacing_mut().item_spacing.x = NAV_BUTTON_GAP;
    let available = ui.available_width() - if app.loading { 28.0 } else { 0.0 };
    let button_width = ((available - NAV_BUTTON_GAP * (NAV_BUTTON_COUNT - 1.0)) / NAV_BUTTON_COUNT)
        .clamp(NAV_BUTTON_MIN_WIDTH, NAV_BUTTON_MAX_WIDTH);
    let nav_width = button_width * NAV_BUTTON_COUNT + NAV_BUTTON_GAP * (NAV_BUTTON_COUNT - 1.0);
    ui.add_space((available - nav_width).max(0.0));

    nav_button(ui, app, "文献库", View::Library, button_width);
    nav_button(ui, app, "导入", View::Upload, button_width);
    nav_button(ui, app, "导出", View::Export, button_width);
    nav_button(ui, app, "设置", View::ProjectManagement, button_width);
    if ui
        .add_sized([button_width, BUTTON_HEIGHT], egui::Button::new("刷新"))
        .clicked()
    {
        app.refresh_projects();
    }
}

fn nav_button(ui: &mut egui::Ui, app: &mut RayviewApp, label: &str, target: View, width: f32) {
    if ui
        .add_sized(
            [width, BUTTON_HEIGHT],
            egui::Button::selectable(app.view == target, label),
        )
        .clicked()
    {
        app.view = target;
    }
}
