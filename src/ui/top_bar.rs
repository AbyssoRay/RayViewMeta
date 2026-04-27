use crate::app::{RayviewApp, View, CONFIRM_DELETE_PROJECT};
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::Panel::top("top_bar")
        .exact_size(64.0)
        .frame(
            egui::Frame::new()
                .fill(theme::HEADER)
                .inner_margin(egui::Margin::symmetric(12, 0)),
        )
        .show_inside(root_ui, |ui| {
            ui.add_space(7.0);
            ui.horizontal_centered(|ui| {
                if let Some(texture) = &app.logo_texture {
                    ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(egui::vec2(112.0, 28.0))
                            .maintain_aspect_ratio(true),
                    );
                    ui.add_space(8.0);
                }
                ui.label(
                    egui::RichText::new("Rayview Meta")
                        .strong()
                        .size(23.0)
                        .color(theme::TEXT),
                );
                ui.add_space(12.0);

                render_project_controls(ui, app);
                ui.separator();

                nav_button(ui, app, "文献库", View::Library);
                nav_button(ui, app, "导入", View::Upload);
                nav_button(ui, app, "导出", View::Export);
                nav_button(ui, app, "设置", View::Settings);
                if ui.button("刷新").clicked() {
                    app.refresh_projects();
                }
                if app.loading {
                    ui.spinner();
                    ui.label(egui::RichText::new("Sync").small().color(theme::CYAN));
                }
            });
            ui.add_space(6.0);
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
        .width(190.0)
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
    ui.menu_button("管理", |ui| {
        ui.set_min_width(300.0);
        ui.label(theme::section_label("Current Library"));
        ui.label(egui::RichText::new(app.current_project_name()).strong());
        ui.add_space(8.0);

        ui.label("重命名文献库");
        ui.text_edit_singleline(&mut app.project_rename_name);
        if ui.button("保存名称").clicked() {
            app.submit_rename_current_project();
        }

        ui.separator();
        ui.label("新建文献库");
        ui.text_edit_singleline(&mut app.new_project_name);
        if ui.button("新建文献库").clicked() {
            app.submit_create_project();
        }

        ui.separator();
        ui.label(egui::RichText::new("删除当前文献库").color(theme::DANGER));
        ui.label(
            egui::RichText::new(format!("输入“{CONFIRM_DELETE_PROJECT}”后删除。"))
                .color(theme::MUTED),
        );
        ui.text_edit_singleline(&mut app.confirm_delete_project);
        if ui
            .add_enabled(
                app.projects.len() > 1
                    && app.confirm_delete_project.trim() == CONFIRM_DELETE_PROJECT,
                egui::Button::new("删除当前文献库"),
            )
            .on_hover_text("删除当前文献库及其中全部文献")
            .clicked()
        {
            app.submit_delete_current_project();
        }
    });
}

fn nav_button(ui: &mut egui::Ui, app: &mut RayviewApp, label: &str, target: View) {
    if ui.selectable_label(app.view == target, label).clicked() {
        app.view = target;
    }
}
