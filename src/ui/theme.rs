pub const BG: egui::Color32 = egui::Color32::from_rgb(10, 10, 12);
pub const HEADER: egui::Color32 = egui::Color32::from_rgb(15, 15, 18);
pub const SURFACE: egui::Color32 = egui::Color32::from_rgb(24, 24, 28);
pub const SURFACE_2: egui::Color32 = egui::Color32::from_rgb(31, 31, 36);
pub const SURFACE_3: egui::Color32 = egui::Color32::from_rgb(39, 39, 45);
pub const LINE: egui::Color32 = egui::Color32::from_rgb(58, 58, 66);
pub const LINE_SOFT: egui::Color32 = egui::Color32::from_rgb(42, 42, 48);
pub const TEXT: egui::Color32 = egui::Color32::WHITE;
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(140, 140, 148);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(210, 184, 116);
pub const CYAN: egui::Color32 = egui::Color32::from_rgb(198, 198, 205);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(224, 96, 96);
pub const SUCCESS: egui::Color32 = egui::Color32::from_rgb(130, 196, 130);
pub const HIGHLIGHT_FG: egui::Color32 = egui::Color32::from_rgb(255, 203, 107);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(
                25.0,
                egui::FontFamily::Name("source_han_serif_sc_bold".into()),
            ),
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
    style.visuals.extreme_bg_color = BG;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, BG);
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
    style.visuals.widgets.active.bg_fill = BG;
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
        egui::Color32::from_rgb(18, 18, 22)
    } else {
        egui::Color32::from_rgb(21, 21, 25)
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
        visuals.widgets.inactive.bg_fill = SURFACE_2;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, LINE);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 26, 28);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.4, DANGER);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(18, 12, 13);
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
