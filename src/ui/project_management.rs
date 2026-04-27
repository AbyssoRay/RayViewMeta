use std::time::{Duration, Instant};

use crate::app::{RayviewApp, View, CONFIRM_DELETE_PROJECT};
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    sync_projects_if_due(app);
    app.ensure_project_management_buffer();

    egui::CentralPanel::default().show_inside(root_ui, |ui| {
        theme::page_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(theme::section_label("Project Library"));
                    ui.heading("项目库管理");
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("立即同步").clicked() {
                        app.last_project_management_sync_at = Some(Instant::now());
                        app.refresh_projects();
                    }
                    if ui.button("返回文献库").clicked() {
                        app.view = View::Library;
                    }
                });
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.columns(2, |columns| {
                        render_project_list(app, &mut columns[0]);
                        render_project_actions(app, &mut columns[1]);
                    });

                    render_presets(app, ui);
                });
        });
    });
}

fn sync_projects_if_due(app: &mut RayviewApp) {
    if app.loading {
        return;
    }
    let should_sync = app
        .last_project_management_sync_at
        .map(|last| last.elapsed() >= Duration::from_secs(3))
        .unwrap_or(true);
    if should_sync {
        app.last_project_management_sync_at = Some(Instant::now());
        app.refresh_projects();
    }
}

fn render_project_list(app: &mut RayviewApp, ui: &mut egui::Ui) {
    theme::panel_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(theme::section_label("Libraries"));
        ui.heading("文献库");
        ui.separator();

        let projects = app.projects.clone();
        for project in projects {
            let selected = app.persisted.selected_project_id == project.id;
            let label = format!("{}  ·  {} 篇", project.name, project.article_count);
            if ui
                .add_sized(
                    [ui.available_width(), 34.0],
                    egui::Button::selectable(selected, label),
                )
                .clicked()
            {
                app.select_project(project.id);
                app.view = View::ProjectManagement;
            }
        }
    });
}

fn render_project_actions(app: &mut RayviewApp, ui: &mut egui::Ui) {
    theme::panel_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(theme::section_label("Current Library"));
        ui.heading(app.current_project_name());
        ui.separator();

        ui.label("重命名文献库");
        ui.add(
            egui::TextEdit::singleline(&mut app.project_rename_name).desired_width(f32::INFINITY),
        );
        if ui.button("保存名称").clicked() {
            app.submit_rename_current_project();
        }

        ui.add_space(14.0);
        ui.separator();
        ui.label("新建文献库");
        ui.add(egui::TextEdit::singleline(&mut app.new_project_name).desired_width(f32::INFINITY));
        if ui.button("新建文献库").clicked() {
            app.submit_create_project();
        }

        ui.add_space(14.0);
        ui.separator();
        ui.label(egui::RichText::new("删除当前文献库").color(theme::DANGER));
        ui.add(
            egui::TextEdit::singleline(&mut app.confirm_delete_project)
                .hint_text(CONFIRM_DELETE_PROJECT)
                .desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(
                app.projects.len() > 1
                    && app.confirm_delete_project.trim() == CONFIRM_DELETE_PROJECT,
                egui::Button::new("删除当前文献库"),
            )
            .clicked()
        {
            app.submit_delete_current_project();
        }
    });
}

fn render_presets(app: &mut RayviewApp, ui: &mut egui::Ui) {
    ui.add_space(10.0);
    render_list_editor(
        ui,
        "Highlight Terms",
        "预设关键词",
        &mut app.new_keyword,
        &mut app.persisted.keywords,
        "添加关键词",
    );
    render_list_editor(
        ui,
        "Label Presets",
        "标签",
        &mut app.new_tag,
        &mut app.persisted.custom_tags,
        "添加标签",
    );
    render_list_editor(
        ui,
        "Exclusion Reasons",
        "排除原因",
        &mut app.new_exclusion_reason,
        &mut app.persisted.exclusion_reasons,
        "添加原因",
    );
}

fn render_list_editor(
    ui: &mut egui::Ui,
    section: &str,
    label: &str,
    input: &mut String,
    values: &mut Vec<String>,
    add_label: &str,
) {
    theme::panel_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(theme::section_label(section));
        ui.heading(label);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(input).desired_width(280.0));
            if ui.button(add_label).clicked() {
                let value = input.trim().to_string();
                if !value.is_empty() && !values.contains(&value) {
                    values.push(value);
                    input.clear();
                }
            }
        });

        let snapshot = values.clone();
        let mut to_remove: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (index, value) in snapshot.iter().enumerate() {
                if theme::removable_chip_button(ui, value).clicked() {
                    to_remove = Some(index);
                }
            }
        });
        if let Some(index) = to_remove {
            values.remove(index);
        }
    });
}
