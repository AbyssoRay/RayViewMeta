use crate::app::{RayviewApp, View};
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top_bar")
        .exact_height(78.0)
        .frame(
            egui::Frame::none()
                .fill(theme::HEADER)
                .inner_margin(egui::Margin::symmetric(12.0, 0.0)),
        )
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal_centered(|ui| {
                if let Some(texture) = &app.logo_texture {
                    ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(egui::vec2(112.0, 28.0))
                            .maintain_aspect_ratio(true),
                    );
                    ui.add_space(6.0);
                }
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Rayview Meta")
                            .strong()
                            .size(23.0)
                            .color(theme::TEXT),
                    );
                    ui.label(
                        egui::RichText::new("Systematic Review Command")
                            .small()
                            .color(theme::CYAN),
                    );
                });
                ui.separator();

                render_project_controls(ui, app);
                ui.separator();

                nav_button(ui, app, "文献库", View::Library);
                nav_button(ui, app, "导入", View::Upload);
                nav_button(ui, app, "导出", View::Export);
                nav_button(ui, app, "设置", View::Settings);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("刷新").clicked() {
                        app.refresh();
                    }
                    if app.loading {
                        ui.spinner();
                        ui.label(egui::RichText::new("Sync").small().color(theme::CYAN));
                    }
                });
            });
            ui.add_space(6.0);
            ui.painter().line_segment(
                [ui.min_rect().left_bottom(), ui.min_rect().right_bottom()],
                egui::Stroke::new(1.0, theme::LINE_SOFT),
            );
        });
}

fn render_project_controls(ui: &mut egui::Ui, app: &mut RayviewApp) {
    ui.label(egui::RichText::new("项目").color(theme::MUTED));
    let selected_text = app.current_project_name();
    let projects = app.projects.clone();
    egui::ComboBox::from_id_salt("project_selector")
        .width(160.0)
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
    ui.add_sized(
        [130.0, 26.0],
        egui::TextEdit::singleline(&mut app.new_project_name).hint_text("新项目名称"),
    );
    if ui.button("新建").clicked() {
        app.submit_create_project();
    }
    if ui
        .add_enabled(app.projects.len() > 1, egui::Button::new("删除"))
        .on_hover_text("删除当前项目及其文献库")
        .clicked()
    {
        app.submit_delete_current_project();
    }
}

fn nav_button(ui: &mut egui::Ui, app: &mut RayviewApp, label: &str, target: View) {
    if ui.selectable_label(app.view == target, label).clicked() {
        app.view = target;
    }
}
