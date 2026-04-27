use crate::app::{RayviewApp, View};
use crate::ui::theme;

const HEADER_HEIGHT: f32 = 96.0;
const BUTTON_HEIGHT: f32 = 36.0;
const LOGO_WIDTH: f32 = 250.0;
const LOGO_HEIGHT: f32 = 66.0;
const NAV_BUTTON_COUNT: f32 = 5.0;
const NAV_BUTTON_GAP: f32 = 14.0;
const NAV_BUTTON_WIDTH: f32 = 90.0;
const HEADER_SIDE_MARGIN: i8 = 56;
const HEADER_GROUP_GAP: f32 = 24.0;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::Panel::top("top_bar")
        .exact_size(HEADER_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(theme::HEADER)
                .inner_margin(egui::Margin::symmetric(HEADER_SIDE_MARGIN, 0)),
        )
        .show_inside(root_ui, |ui| {
            ui.add_space(12.0);
            ui.horizontal_centered(|ui| {
                let nav_width =
                    NAV_BUTTON_COUNT * NAV_BUTTON_WIDTH + (NAV_BUTTON_COUNT - 1.0) * NAV_BUTTON_GAP;
                let right_width = nav_width;
                let left_width = (ui.available_width() - right_width - HEADER_GROUP_GAP).max(0.0);

                ui.allocate_ui_with_layout(
                    egui::vec2(left_width, LOGO_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| render_brand_group(ui, app),
                );
                ui.add_space(HEADER_GROUP_GAP);
                ui.allocate_ui_with_layout(
                    egui::vec2(right_width, LOGO_HEIGHT),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| render_toolbar_buttons(ui, app),
                );
            });
            ui.add_space(8.0);
            ui.painter().line_segment(
                [ui.min_rect().left_bottom(), ui.min_rect().right_bottom()],
                egui::Stroke::new(1.0, theme::LINE_SOFT),
            );
        });
}

fn render_brand_group(ui: &mut egui::Ui, app: &RayviewApp) {
    if let Some(texture) = &app.logo_texture {
        ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(egui::vec2(LOGO_WIDTH, LOGO_HEIGHT))
                .maintain_aspect_ratio(true),
        );
        ui.add_space(20.0);
    }

    ui.label(
        egui::RichText::new("Rayview Meta")
            .strong()
            .size(26.0)
            .color(theme::TEXT),
    );
    ui.add_space(24.0);

    let label = format!("当前项目：{}", app.current_project_name());
    ui.add_sized(
        [ui.available_width(), BUTTON_HEIGHT],
        egui::Label::new(egui::RichText::new(label).size(15.0).color(theme::TEXT)).truncate(),
    )
    .on_hover_text(app.current_project_name());
}

fn render_toolbar_buttons(ui: &mut egui::Ui, app: &mut RayviewApp) {
    ui.spacing_mut().item_spacing.x = NAV_BUTTON_GAP;
    if ui
        .add_sized([NAV_BUTTON_WIDTH, BUTTON_HEIGHT], egui::Button::new("刷新"))
        .clicked()
    {
        app.refresh_projects();
    }
    nav_button(ui, app, "设置", View::ProjectManagement);
    nav_button(ui, app, "导出", View::Export);
    nav_button(ui, app, "导入", View::Upload);
    nav_button(ui, app, "文献库", View::Library);
}

fn nav_button(ui: &mut egui::Ui, app: &mut RayviewApp, label: &str, target: View) {
    if ui
        .add_sized(
            [NAV_BUTTON_WIDTH, BUTTON_HEIGHT],
            egui::Button::selectable(app.view == target, label),
        )
        .clicked()
    {
        app.view = target;
    }
}
