use crate::app::{RayviewApp, DEFAULT_SERVER_URL};
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        theme::page_frame().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(theme::section_label("Config / Review Protocol"));
                    ui.heading("项目设置");
                    ui.separator();

                    theme::panel_frame().show(ui, |ui| {
                        ui.label(theme::section_label("Server"));
                        ui.label("服务端地址（包含协议与端口）");
                        ui.text_edit_singleline(&mut app.settings_url_buf);
                        ui.horizontal(|ui| {
                            if ui.button("应用并刷新").clicked() {
                                let url = app.settings_url_buf.trim().to_string();
                                if !url.is_empty() {
                                    app.persisted.server_url = url.clone();
                                    app.api.set_base_url(url);
                                    app.set_status("服务端地址已更新，正在刷新");
                                    app.refresh_projects();
                                }
                            }
                            if ui.button("重置默认服务器").clicked() {
                                let default = DEFAULT_SERVER_URL.to_string();
                                app.settings_url_buf = default.clone();
                                app.persisted.server_url = default.clone();
                                app.api.set_base_url(default);
                                app.refresh_projects();
                            }
                        });
                    });

                    theme::panel_frame().show(ui, |ui| {
                        ui.label(theme::section_label("Abstract Highlight"));
                        ui.label("预设关键词会在所有文献摘要中高亮。 ");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut app.new_keyword);
                            if ui.button("添加关键词").clicked() {
                                let keyword = app.new_keyword.trim().to_string();
                                if !keyword.is_empty() && !app.persisted.keywords.contains(&keyword)
                                {
                                    app.persisted.keywords.push(keyword);
                                    app.new_keyword.clear();
                                }
                            }
                        });
                        render_remove_list(ui, &mut app.persisted.keywords);
                    });

                    theme::panel_frame().show(ui, |ui| {
                        ui.label(theme::section_label("Label Presets"));
                        ui.label("这些标签会在详情页作为快速标签出现。 ");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut app.new_tag);
                            if ui.button("添加标签").clicked() {
                                let tag = app.new_tag.trim().to_string();
                                if !tag.is_empty() && !app.persisted.custom_tags.contains(&tag) {
                                    app.persisted.custom_tags.push(tag);
                                    app.new_tag.clear();
                                }
                            }
                        });
                        render_remove_list(ui, &mut app.persisted.custom_tags);
                    });

                    theme::panel_frame().show(ui, |ui| {
                        ui.label(theme::section_label("Exclusion Reasons"));
                        ui.label("排除原因会在详情页一键填写，适合统一筛选口径。 ");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut app.new_exclusion_reason);
                            if ui.button("添加原因").clicked() {
                                let reason = app.new_exclusion_reason.trim().to_string();
                                if !reason.is_empty()
                                    && !app.persisted.exclusion_reasons.contains(&reason)
                                {
                                    app.persisted.exclusion_reasons.push(reason);
                                    app.new_exclusion_reason.clear();
                                }
                            }
                        });
                        render_remove_list(ui, &mut app.persisted.exclusion_reasons);
                    });
                });
        });
    });
}

fn render_remove_list(ui: &mut egui::Ui, values: &mut Vec<String>) {
    let snapshot = values.clone();
    let mut remove: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        for (index, value) in snapshot.iter().enumerate() {
            if theme::removable_chip_button(ui, value)
                .on_hover_text("单击删除")
                .clicked()
            {
                remove = Some(index);
            }
        }
    });
    if let Some(index) = remove {
        values.remove(index);
    }
}
