pub const BG: egui::Color32 = egui::Color32::from_rgb(30, 30, 30);
pub const HEADER: egui::Color32 = egui::Color32::from_rgb(37, 37, 38);
pub const SURFACE: egui::Color32 = egui::Color32::from_rgb(45, 45, 48);
pub const SURFACE_2: egui::Color32 = egui::Color32::from_rgb(51, 51, 51);
pub const SURFACE_3: egui::Color32 = egui::Color32::from_rgb(62, 62, 64);
pub const LINE: egui::Color32 = egui::Color32::from_rgb(82, 82, 86);
pub const LINE_SOFT: egui::Color32 = egui::Color32::from_rgb(63, 63, 70);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(212, 212, 212);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(156, 156, 156);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(190, 190, 190);
pub const CYAN: egui::Color32 = egui::Color32::from_rgb(180, 180, 180);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(210, 92, 92);
pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(125, 180, 125);
pub const HIGHLIGHT_BG: egui::Color32 = egui::Color32::from_rgb(76, 76, 76);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(25.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(16.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(15.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(14.0, egui::FontFamily::Monospace),
        ),
    ]
    .into();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(24, 24, 24);
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, TEXT);
    style.visuals.hyperlink_color = CYAN;

    style.visuals.widgets.noninteractive.bg_fill = SURFACE;
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE_SOFT);
    style.visuals.widgets.inactive.bg_fill = SURFACE_2;
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, LINE);
    style.visuals.widgets.hovered.bg_fill = SURFACE_3;
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(74, 74, 76);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);

    style.spacing.item_spacing = egui::vec2(9.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.window_margin = egui::Margin::same(12);
    ctx.set_global_style(style);
}

pub fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(egui::Stroke::new(1.0, LINE_SOFT))
        .inner_margin(egui::Margin::same(14))
        .outer_margin(egui::Margin::same(4))
}

pub fn page_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(BG)
        .inner_margin(egui::Margin::symmetric(20, 18))
}

pub fn row_frame(selected: bool) -> egui::Frame {
    let stroke = if selected {
        egui::Stroke::new(1.5, ACCENT)
    } else {
        egui::Stroke::new(1.0, LINE_SOFT)
    };
    let fill = if selected {
        egui::Color32::from_rgb(56, 56, 58)
    } else {
        egui::Color32::from_rgb(38, 38, 40)
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .outer_margin(egui::Margin::symmetric(0, 3))
}

pub fn section_label(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into())
        .strong()
        .small()
        .color(CYAN)
}

pub fn chip(text: impl Into<String>, color: egui::Color32) -> egui::RichText {
    egui::RichText::new(text.into())
        .small()
        .background_color(dim(color))
        .color(TEXT)
}

pub fn removable_chip_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let text = text.into();
    ui.scope(|ui| {
        let visuals = &mut ui.style_mut().visuals;
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(27, 32, 39);
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, LINE);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(48, 30, 31);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.4, DANGER);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(60, 32, 32);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.4, DANGER);
        ui.style_mut().spacing.button_padding = egui::vec2(10.0, 4.0);
        ui.add(egui::Button::new(
            egui::RichText::new(text).small().color(TEXT),
        ))
    })
    .inner
}

fn dim(color: egui::Color32) -> egui::Color32 {
    egui::Color32::from_rgb(
        ((color.r() as f32) * 0.32) as u8,
        ((color.g() as f32) * 0.32) as u8,
        ((color.b() as f32) * 0.32) as u8,
    )
}
