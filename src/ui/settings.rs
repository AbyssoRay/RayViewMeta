use crate::app::{RayviewApp, DEFAULT_SERVER_URL};
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::CentralPanel::default().show_inside(root_ui, |ui| {
        theme::page_frame().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(theme::section_label("Config / Review Protocol"));
                    ui.heading("项目设置");
                    ui.separator();

                    theme::panel_frame().show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(theme::section_label("Server"));
                        ui.label("服务端地址（包含协议与端口）");
                        ui.add(
                            egui::TextEdit::singleline(&mut app.settings_url_buf)
                                .desired_width(f32::INFINITY),
                        );
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
                });
        });
    });
}
