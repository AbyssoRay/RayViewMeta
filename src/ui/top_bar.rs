use crate::app::{RayviewApp, View};
use crate::ui::theme;

const HEADER_HEIGHT: f32 = 76.0;
const BUTTON_HEIGHT: f32 = 36.0;
const LOGO_WIDTH: f32 = 150.0;
const LOGO_HEIGHT: f32 = 38.0;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::Panel::top("top_bar")
        .exact_size(HEADER_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(theme::HEADER)
                .inner_margin(egui::Margin::symmetric(16, 0)),
        )
        .show_inside(root_ui, |ui| {
            ui.add_space(10.0);
            ui.horizontal_centered(|ui| {
                if let Some(texture) = &app.logo_texture {
                    ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(egui::vec2(LOGO_WIDTH, LOGO_HEIGHT))
                            .maintain_aspect_ratio(true),
                    );
                    ui.add_space(10.0);
                }
                ui.label(
                    egui::RichText::new("Rayview Meta")
                        .strong()
                        .size(24.0)
                        .color(theme::TEXT),
                );
                ui.add_space(14.0);

                render_project_controls(ui, app);
                ui.add_space(12.0);

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

fn render_project_controls(ui: &mut egui::Ui, app: &mut RayviewApp) {
    app.ensure_project_management_buffer();
    ui.label(egui::RichText::new("项目库").color(theme::MUTED));
    let selected_text = app.current_project_name();
    let projects = app.projects.clone();
    egui::ComboBox::from_id_salt("project_selector")
        .width(210.0)
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            for project in projects {
                let label = format!("{} ({})", project.name, project.article_count);
                if ui
                    .selectable_label(app.persisted.selected_project_id == project.id, label)
                    .clicked()
                {
                    app.select_project(project.id);
                }
            }
        });
}

fn render_toolbar_buttons(ui: &mut egui::Ui, app: &mut RayviewApp) {
    let button_count = 6.0;
    let gap = 8.0;
    ui.spacing_mut().item_spacing.x = gap;
    let available = ui.available_width() - if app.loading { 28.0 } else { 0.0 };
    let button_width = ((available - gap * (button_count - 1.0)) / button_count).max(72.0);

    nav_button(ui, app, "文献库", View::Library, button_width);
    nav_button(ui, app, "导入", View::Upload, button_width);
    nav_button(ui, app, "导出", View::Export, button_width);
    nav_button(ui, app, "项目管理", View::ProjectManagement, button_width);
    nav_button(ui, app, "设置", View::Settings, button_width);
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
