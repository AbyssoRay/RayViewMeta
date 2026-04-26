use image::imageops::FilterType;

const ICON_BYTES: &[u8] = include_bytes!("images/icon.png");
const LOGO_BYTES: &[u8] = include_bytes!("images/logo.png");

pub fn app_icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(ICON_BYTES).ok()?;
    let image = if image.width() < 256 || image.height() < 256 {
        image.resize_exact(256, 256, FilterType::Lanczos3)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    Some(egui::IconData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

pub fn load_logo_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(LOGO_BYTES).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("rayview_logo", color_image, egui::TextureOptions::LINEAR))
}
