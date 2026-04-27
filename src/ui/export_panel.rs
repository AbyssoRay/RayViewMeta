use std::path::PathBuf;

use anyhow::{anyhow, Result};
use rust_xlsxwriter::{Format, Workbook};
use shared::{Article, ArticleSource, Decision};

use crate::app::RayviewApp;
use crate::ui::theme;

const CONFIRM_DELETE_ALL: &str = "删除全部文献";
const CONFIRM_DELETE_NOT_INCLUDED: &str = "删除所有未标为纳入的文献";
const CONFIRM_CLEAR_FILTERS: &str = "清空全部筛选";

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::CentralPanel::default().show_inside(root_ui, |ui| {
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
        ui.label(theme::section_label("Library Export"));
        ui.heading("导出当前文献库");
        let total_count = app.articles.len();
        ui.label(
            egui::RichText::new(format!("当前文献库：{total_count} 篇文献")).color(theme::MUTED),
        );

        if ui.button("导出全部文献到 Excel 表格").clicked() {
            if total_count == 0 {
                app.set_status("当前文献库没有文献可导出");
                return;
            }
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Excel 工作簿", &["xlsx"])
                .set_file_name("rayview_library_articles.xlsx")
                .save_file()
            {
                match export_library_articles(&app.articles, path) {
                    Ok(count) => app.set_status(format!("已导出 {count} 篇文献")),
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

fn export_library_articles(articles: &[Article], path: PathBuf) -> Result<usize> {
    if articles.is_empty() {
        return Err(anyhow!("当前文献库没有文献可导出"));
    }

    let path = ensure_xlsx_extension(path);
    let mut workbook = Workbook::new();
    let header_format = Format::new().set_bold();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Library")?;
    worksheet.set_column_width(0, 80)?;
    worksheet.set_column_width(1, 36)?;
    worksheet.set_column_width(2, 120)?;
    worksheet.set_column_width(3, 18)?;
    worksheet.set_column_width(4, 32)?;
    worksheet.set_column_width(5, 60)?;
    worksheet.set_column_width(6, 42)?;
    worksheet.set_column_width(7, 32)?;
    worksheet.set_column_width(8, 12)?;
    worksheet.set_column_width(9, 18)?;
    worksheet.set_column_width(10, 16)?;
    worksheet.set_column_width(11, 32)?;
    worksheet.set_column_width(12, 12)?;
    worksheet.set_column_width(13, 42)?;
    worksheet.write_string_with_format(0, 0, "Title", &header_format)?;
    worksheet.write_string_with_format(0, 1, "DOI", &header_format)?;
    worksheet.write_string_with_format(0, 2, "Reference", &header_format)?;
    worksheet.write_string_with_format(0, 3, "Decision", &header_format)?;
    worksheet.write_string_with_format(0, 4, "Exclusion Reason", &header_format)?;
    worksheet.write_string_with_format(0, 5, "Notes", &header_format)?;
    worksheet.write_string_with_format(0, 6, "Authors", &header_format)?;
    worksheet.write_string_with_format(0, 7, "Journal", &header_format)?;
    worksheet.write_string_with_format(0, 8, "Year", &header_format)?;
    worksheet.write_string_with_format(0, 9, "PMID", &header_format)?;
    worksheet.write_string_with_format(0, 10, "Source", &header_format)?;
    worksheet.write_string_with_format(0, 11, "Tags", &header_format)?;
    worksheet.write_string_with_format(0, 12, "Starred", &header_format)?;
    worksheet.write_string_with_format(0, 13, "Keywords", &header_format)?;

    for (index, article) in articles.iter().enumerate() {
        let row = (index + 1) as u32;
        worksheet.write_string(row, 0, &article.title)?;
        worksheet.write_string(row, 1, article.doi.as_deref().unwrap_or(""))?;
        let reference = format_reference(article);
        worksheet.write_string(row, 2, &reference)?;
        worksheet.write_string(row, 3, article.decision.label())?;
        worksheet.write_string(row, 4, &article.exclusion_reason)?;
        worksheet.write_string(row, 5, &article.notes)?;
        let authors = article.authors.join("; ");
        worksheet.write_string(row, 6, &authors)?;
        worksheet.write_string(row, 7, article.journal.as_deref().unwrap_or(""))?;
        let year = article
            .year
            .map(|year| year.to_string())
            .unwrap_or_default();
        worksheet.write_string(row, 8, &year)?;
        worksheet.write_string(row, 9, article.pmid.as_deref().unwrap_or(""))?;
        worksheet.write_string(row, 10, source_label(article.source))?;
        let tags = article.tags.join("; ");
        worksheet.write_string(row, 11, &tags)?;
        worksheet.write_string(row, 12, if article.starred { "yes" } else { "no" })?;
        let keywords = article.keywords.join("; ");
        worksheet.write_string(row, 13, &keywords)?;
    }

    workbook
        .save(&path)
        .map_err(|error| anyhow!("无法写入 {}: {error}", path.display()))?;
    Ok(articles.len())
}

fn source_label(source: ArticleSource) -> &'static str {
    match source {
        ArticleSource::Manual => "Manual",
        ArticleSource::Pdf => "PDF",
        ArticleSource::Pubmed => "PubMed",
        ArticleSource::Web => "Web",
    }
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

fn format_reference(article: &Article) -> String {
    let year = article
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| "n.d.".to_string());
    let mut parts = Vec::new();

    if article.authors.is_empty() {
        parts.push(sentence_part(&article.title));
        parts.push(format!("({year})."));
    } else {
        parts.push(sentence_part(&article.authors.join(", ")));
        parts.push(format!("({year})."));
        parts.push(sentence_part(&article.title));
    }

    if let Some(journal) = article
        .journal
        .as_deref()
        .filter(|journal| !journal.trim().is_empty())
    {
        parts.push(sentence_part(journal));
    }
    if let Some(doi) = article.doi.as_deref().and_then(doi_url) {
        parts.push(doi);
    }

    parts.join(" ")
}

fn sentence_part(value: &str) -> String {
    let value = value.trim();
    if value.ends_with(['.', '?', '!', '。', '？', '！']) {
        value.to_string()
    } else {
        format!("{value}.")
    }
}

fn doi_url(doi: &str) -> Option<String> {
    let mut value = doi.trim();
    if value.is_empty() {
        return None;
    }
    if value.to_ascii_lowercase().starts_with("doi:") {
        value = value[4..].trim();
    }
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
    ] {
        if value.to_ascii_lowercase().starts_with(prefix) {
            value = &value[prefix.len()..];
            break;
        }
    }
    Some(format!("https://doi.org/{value}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use shared::{ArticleSource, FieldVersions};

    use super::*;

    #[test]
    fn exports_library_articles_as_real_xlsx() {
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

        let count = export_library_articles(&articles, path).unwrap();
        let bytes = fs::read(&xlsx_path).unwrap();

        assert_eq!(count, 2);
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

    #[test]
    fn formats_reference_with_common_citation_shape() {
        let mut article = article("included", Decision::Include, Some("10.1234/example"));
        article.authors = vec!["Jane Smith".to_string(), "Wei Li".to_string()];
        article.journal = Some("Journal of Examples".to_string());
        article.year = Some(2025);

        assert_eq!(
            format_reference(&article),
            "Jane Smith, Wei Li. (2025). Title included. Journal of Examples. https://doi.org/10.1234/example"
        );
    }

    #[test]
    fn formats_reference_without_authors() {
        let mut article = article("included", Decision::Include, None);
        article.year = Some(2024);

        assert_eq!(format_reference(&article), "Title included. (2024).");
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
            keywords: Vec::new(),
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
