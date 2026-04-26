use std::path::PathBuf;

use anyhow::{anyhow, Result};
use rust_xlsxwriter::{Format, Workbook};
use shared::{Article, Decision};

use crate::app::RayviewApp;
use crate::ui::theme;

const CONFIRM_DELETE_ALL: &str = "删除全部文献";
const CONFIRM_DELETE_NOT_INCLUDED: &str = "删除所有未标为纳入的文献";
const CONFIRM_CLEAR_FILTERS: &str = "清空全部筛选";

pub fn show(app: &mut RayviewApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        theme::page_frame().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(theme::section_label("Export / Library Control"));
                    ui.heading("导出与批量操作");
                    ui.separator();

                    render_export_panel(app, ui);
                    render_filter_panel(app, ui);
                    render_delete_panel(app, ui);
                });
        });
    });
}

fn render_export_panel(app: &mut RayviewApp, ui: &mut egui::Ui) {
    theme::panel_frame().show(ui, |ui| {
        ui.label(theme::section_label("Included Articles"));
        ui.heading("导出纳入文献");
        let included_count = app
            .articles
            .iter()
            .filter(|article| article.decision == Decision::Include)
            .count();
        ui.label(
            egui::RichText::new(format!("当前标为“纳入”的文献：{included_count} 篇"))
                .color(theme::MUTED),
        );

        if ui.button("导出标题和 DOI 到 Excel 表格").clicked() {
            if included_count == 0 {
                app.set_status("没有标为“纳入”的文献可导出");
                return;
            }
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Excel 工作簿", &["xlsx"])
                .set_file_name("rayview_included_articles.xlsx")
                .save_file()
            {
                match export_included_articles(&app.articles, path) {
                    Ok(count) => app.set_status(format!("已导出 {count} 篇纳入文献")),
                    Err(error) => app.set_status(format!("导出失败: {error}")),
                }
            }
        }
    });
}

fn render_filter_panel(app: &mut RayviewApp, ui: &mut egui::Ui) {
    theme::panel_frame().show(ui, |ui| {
        ui.label(theme::section_label("Filter Reset"));
        ui.heading("清空全部筛选");
        ui.label(
            egui::RichText::new(format!(
                "输入“{CONFIRM_CLEAR_FILTERS}”后执行。此操作不会删除文献。"
            ))
            .color(theme::MUTED),
        );
        ui.text_edit_singleline(&mut app.confirm_clear_filters);
        if ui
            .add_enabled(
                app.confirm_clear_filters.trim() == CONFIRM_CLEAR_FILTERS,
                egui::Button::new("清空全部筛选"),
            )
            .clicked()
        {
            app.clear_filters();
            app.confirm_clear_filters.clear();
            app.set_status("已清空全部筛选");
        }
    });
}

fn render_delete_panel(app: &mut RayviewApp, ui: &mut egui::Ui) {
    let total = app.articles.len();
    let not_included = app
        .articles
        .iter()
        .filter(|article| article.decision != Decision::Include)
        .count();

    theme::panel_frame().show(ui, |ui| {
        ui.label(theme::section_label("Danger Zone"));
        ui.heading("批量删除");
        ui.label(
            egui::RichText::new("以下操作会删除服务端数据，请确认后再执行。").color(theme::DANGER),
        );
        ui.separator();

        ui.label(format!("全部文献：{total} 篇"));
        ui.label(
            egui::RichText::new(format!("输入“{CONFIRM_DELETE_ALL}”后删除全部文献。"))
                .color(theme::MUTED),
        );
        ui.text_edit_singleline(&mut app.confirm_delete_all);
        if ui
            .add_enabled(
                app.confirm_delete_all.trim() == CONFIRM_DELETE_ALL && total > 0,
                egui::Button::new("删除全部文献"),
            )
            .clicked()
        {
            let targets = app
                .articles
                .iter()
                .map(|article| (article.id.clone(), article.title.clone()))
                .collect::<Vec<_>>();
            app.confirm_delete_all.clear();
            app.submit_delete_many(targets, "删除全部文献");
        }

        ui.add_space(12.0);
        ui.separator();
        ui.label(format!("未标为“纳入”的文献：{not_included} 篇"));
        ui.label(
            egui::RichText::new(format!(
                "输入“{CONFIRM_DELETE_NOT_INCLUDED}”后删除所有未纳入文献。"
            ))
            .color(theme::MUTED),
        );
        ui.text_edit_singleline(&mut app.confirm_delete_not_included);
        if ui
            .add_enabled(
                app.confirm_delete_not_included.trim() == CONFIRM_DELETE_NOT_INCLUDED
                    && not_included > 0,
                egui::Button::new("删除所有未标为纳入的文献"),
            )
            .clicked()
        {
            let targets = app
                .articles
                .iter()
                .filter(|article| article.decision != Decision::Include)
                .map(|article| (article.id.clone(), article.title.clone()))
                .collect::<Vec<_>>();
            app.confirm_delete_not_included.clear();
            app.submit_delete_many(targets, "删除未纳入文献");
        }
    });
}

fn export_included_articles(articles: &[Article], path: PathBuf) -> Result<usize> {
    let included = articles
        .iter()
        .filter(|article| article.decision == Decision::Include)
        .collect::<Vec<_>>();
    if included.is_empty() {
        return Err(anyhow!("没有标为“纳入”的文献可导出"));
    }

    let path = ensure_xlsx_extension(path);
    let mut workbook = Workbook::new();
    let header_format = Format::new().set_bold();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Included")?;
    worksheet.set_column_width(0, 80)?;
    worksheet.set_column_width(1, 36)?;
    worksheet.write_string_with_format(0, 0, "Title", &header_format)?;
    worksheet.write_string_with_format(0, 1, "DOI", &header_format)?;

    for (index, article) in included.iter().enumerate() {
        let row = (index + 1) as u32;
        worksheet.write_string(row, 0, &article.title)?;
        worksheet.write_string(row, 1, article.doi.as_deref().unwrap_or(""))?;
    }

    workbook
        .save(&path)
        .map_err(|error| anyhow!("无法写入 {}: {error}", path.display()))?;
    Ok(included.len())
}

fn ensure_xlsx_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"))
    {
        path
    } else {
        path.with_extension("xlsx")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use shared::{ArticleSource, FieldVersions};

    use super::*;

    #[test]
    fn exports_included_articles_as_real_xlsx() {
        let path = std::env::temp_dir().join(format!(
            "rayview_export_test_{}.xls",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let xlsx_path = path.with_extension("xlsx");
        let articles = vec![
            article("included", Decision::Include, Some("10.1234/example")),
            article("excluded", Decision::Exclude, Some("10.9999/example")),
        ];

        let count = export_included_articles(&articles, path).unwrap();
        let bytes = fs::read(&xlsx_path).unwrap();

        assert_eq!(count, 1);
        assert!(bytes.starts_with(b"PK"));
        fs::remove_file(xlsx_path).unwrap();
    }

    #[test]
    fn ensure_xlsx_extension_replaces_other_extensions() {
        assert_eq!(
            ensure_xlsx_extension(PathBuf::from("included.xls")),
            PathBuf::from("included.xlsx")
        );
        assert_eq!(
            ensure_xlsx_extension(PathBuf::from("included.xlsx")),
            PathBuf::from("included.xlsx")
        );
    }

    fn article(id: &str, decision: Decision, doi: Option<&str>) -> Article {
        Article {
            id: id.to_string(),
            title: format!("Title {id}"),
            abstract_text: "Abstract text".to_string(),
            authors: Vec::new(),
            journal: None,
            year: None,
            doi: doi.map(str::to_string),
            pmid: None,
            source: ArticleSource::Manual,
            tags: Vec::new(),
            starred: false,
            exclusion_reason: String::new(),
            decision,
            notes: String::new(),
            created_at: 0,
            updated_at: 0,
            version: 0,
            field_versions: FieldVersions::default(),
        }
    }
}
