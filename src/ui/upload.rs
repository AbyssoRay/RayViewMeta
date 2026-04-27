use std::collections::HashSet;

use shared::{ArticleSource, NewArticle};

use crate::api::ApiClient;
use crate::app::RayviewApp;
use crate::tasks::{FailureReport, TaskMsg};
use crate::ui::theme;
use anyhow::anyhow;

pub fn show(app: &mut RayviewApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        theme::page_frame().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(theme::section_label("Ingest / Source Acquisition"));
                    ui.heading("导入文献");
                    ui.separator();

                    theme::panel_frame().show(ui, |ui| {
                        ui.label(theme::section_label("PDF Extraction"));
                        ui.heading("上传 PDF");
                        ui.label(
                            egui::RichText::new(
                                "只从 PDF 文本层识别 DOI，再访问期刊网页抓取标题、摘要和关键词；可一次选择多个文件。",
                            )
                                .color(theme::MUTED),
                        );
                        if ui.button("选择 PDF 文件").clicked() {
                            if let Some(paths) = rfd::FileDialog::new()
                                .add_filter("PDF", &["pdf"])
                                .pick_files()
                            {
                                spawn_pdf_import(app, paths);
                            }
                        }
                    });

                    theme::panel_frame().show(ui, |ui| {
                        ui.label(theme::section_label("Reference Links"));
                        ui.heading("链接批量导入");
                        ui.label(
                            egui::RichText::new(
                                "粘贴 PMID、PubMed 链接、DOI、doi.org 链接或期刊论文网页，系统会抓取标题、摘要和关键词。",
                            )
                            .color(theme::MUTED),
                        );
                        ui.add(
                            egui::TextEdit::multiline(&mut app.pubmed_input)
                                .desired_rows(8)
                                .desired_width(f32::INFINITY)
                                .hint_text(
                                    "https://pubmed.ncbi.nlm.nih.gov/12345678/\n\
                                     23456789\n\
                                     10.1016/j.cell.2020.01.001\n\
                                     https://doi.org/10.1038/s41586-024-00000-0\n\
                                     https://www.nature.com/articles/s41586-024-00000-0",
                                ),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("识别并导入").clicked() {
                                let parsed = parse_reference_input(&app.pubmed_input);
                                if parsed.pmids.is_empty() && parsed.article_inputs.is_empty() {
                                    let reason = parsed
                                        .rejected
                                        .first()
                                        .map(|failure| failure.reason.as_str())
                                        .unwrap_or("请输入 PMID、DOI 或论文网页链接");
                                    app.set_status(format!("未识别到可用文献链接：{reason}"));
                                    app.set_failure_report("导入失败明细", parsed.rejected);
                                } else {
                                    spawn_reference_import(
                                        app,
                                        parsed.pmids,
                                        parsed.article_inputs,
                                        parsed.rejected,
                                    );
                                }
                            }
                            if ui.button("清空输入").clicked() {
                                app.pubmed_input.clear();
                            }
                        });
                    });

                    theme::panel_frame().show(ui, |ui| {
                        ui.label(theme::section_label("Manual Record"));
                        ui.heading("手动录入");
                        ui.label("标题");
                        ui.text_edit_singleline(&mut app.manual_title);
                        ui.label("摘要");
                        ui.add(
                            egui::TextEdit::multiline(&mut app.manual_abstract)
                                .desired_rows(6)
                                .desired_width(f32::INFINITY),
                        );
                        if ui.button("添加到服务端").clicked() {
                            if app.manual_title.trim().is_empty() {
                                app.set_status("标题不能为空");
                            } else {
                                let payload = NewArticle {
                                    title: app.manual_title.trim().to_string(),
                                    abstract_text: app.manual_abstract.trim().to_string(),
                                    authors: Vec::new(),
                                    journal: None,
                                    year: None,
                                    doi: None,
                                    pmid: None,
                                    keywords: Vec::new(),
                                    source: ArticleSource::Manual,
                                };
                                app.manual_title.clear();
                                app.manual_abstract.clear();
                                spawn_create_one(app, payload);
                            }
                        }
                    });
                });
        });
    });
}

#[derive(Default)]
struct ParsedReferenceInput {
    pmids: Vec<String>,
    article_inputs: Vec<String>,
    rejected: Vec<FailureReport>,
}

fn parse_reference_input(input: &str) -> ParsedReferenceInput {
    let mut parsed = ParsedReferenceInput::default();
    let mut seen_pmids = HashSet::new();
    let mut seen_articles = HashSet::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let mut accepted_on_line = false;
        for token in
            line.split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';' | '，' | '；'))
        {
            let token = token.trim();
            if token.is_empty() || token.eq_ignore_ascii_case("pmid") {
                continue;
            }
            if let Some(pmid) = crate::pubmed::parse_pubmed_token(token) {
                accepted_on_line = true;
                if seen_pmids.insert(pmid.clone()) {
                    parsed.pmids.push(pmid);
                }
                continue;
            }
            if crate::doi::is_supported_article_input(token) {
                accepted_on_line = true;
                let key = crate::doi::canonical_input_key(token);
                if seen_articles.insert(key) {
                    parsed.article_inputs.push(token.to_string());
                }
            }
        }
        if !accepted_on_line {
            parsed.rejected.push(FailureReport::new(
                "文献链接输入",
                format!("无法识别 PMID、DOI 或论文网页链接: {line}"),
            ));
        }
    }

    parsed
}

fn spawn_pdf_import(app: &mut RayviewApp, paths: Vec<std::path::PathBuf>) {
    let api = app.api.clone();
    app.loading = true;
    app.set_status(format!("正在解析 {} 个 PDF", paths.len()));
    app.bus.spawn(move |tx| {
        let mut payloads: Vec<NewArticle> = Vec::new();
        let mut failures: Vec<FailureReport> = Vec::new();
        for path in &paths {
            match crate::pdf::extract_from_pdf(path) {
                Ok(article) => payloads.push(article),
                Err(error) => {
                    failures.push(FailureReport::new(
                        path.display().to_string(),
                        error.to_string(),
                    ));
                }
            }
        }
        if payloads.is_empty() {
            let detail = failures
                .first()
                .map(|failure| failure.reason.clone())
                .unwrap_or_else(|| "未找到可导入 PDF".to_string());
            if !failures.is_empty() {
                let _ = tx.send(TaskMsg::ImportFailures(failures));
            }
            let _ = tx.send(TaskMsg::Imported(Err(anyhow!("未导入任何 PDF。{detail}"))));
            return;
        }
        match upload_all(&api, payloads) {
            UploadResult {
                articles,
                failures: upload_failures,
            } if !articles.is_empty() => {
                failures.extend(upload_failures);
                let _ = tx.send(TaskMsg::Imported(Ok(articles)));
            }
            UploadResult {
                failures: upload_failures,
                ..
            } => {
                failures.extend(upload_failures);
                let detail = failures
                    .first()
                    .map(|failure| failure.reason.clone())
                    .unwrap_or_else(|| "上传失败".to_string());
                let _ = tx.send(TaskMsg::Imported(Err(anyhow!(detail))));
            }
        }
        if !failures.is_empty() {
            let _ = tx.send(TaskMsg::ImportFailures(failures));
        }
    });
}

fn spawn_reference_import(
    app: &mut RayviewApp,
    pmids: Vec<String>,
    article_inputs: Vec<String>,
    rejected: Vec<FailureReport>,
) {
    let api = app.api.clone();
    app.loading = true;
    let mut parts = Vec::new();
    if !pmids.is_empty() {
        parts.push(format!("{} 个 PMID", pmids.len()));
    }
    if !article_inputs.is_empty() {
        parts.push(format!("{} 个 DOI/网页链接", article_inputs.len()));
    }
    let suffix = if rejected.is_empty() {
        String::new()
    } else {
        format!("，已忽略 {} 条无法识别的输入", rejected.len())
    };
    app.set_status(format!("正在抓取 {}{suffix}", parts.join("、")));
    app.bus.spawn(move |tx| {
        let mut failures = rejected;
        let mut payloads = Vec::new();
        if !pmids.is_empty() {
            match crate::pubmed::fetch_pubmed_with_failures(&pmids) {
                Ok(fetch) => {
                    failures.extend(fetch.failures.into_iter().map(|failure| {
                        FailureReport::new(format!("PMID {}", failure.pmid), failure.reason)
                    }));
                    payloads.extend(fetch.articles);
                }
                Err(error) => {
                    let reason = error.to_string();
                    failures.extend(
                        pmids
                            .iter()
                            .map(|pmid| FailureReport::new(format!("PMID {pmid}"), reason.clone())),
                    );
                }
            }
        }
        for input in article_inputs {
            match crate::doi::fetch_article_from_input(&input, ArticleSource::Web) {
                Ok(article) => payloads.push(article),
                Err(error) => failures.push(FailureReport::new(input, error.to_string())),
            }
        }

        if payloads.is_empty() {
            let detail = failures
                .first()
                .map(|failure| failure.reason.clone())
                .unwrap_or_else(|| "未找到可导入文献".to_string());
            if !failures.is_empty() {
                let _ = tx.send(TaskMsg::ImportFailures(failures));
            }
            let _ = tx.send(TaskMsg::Imported(Err(anyhow!("未导入任何文献。{detail}"))));
            return;
        }

        match upload_all(&api, payloads) {
            UploadResult {
                articles,
                failures: upload_failures,
            } if !articles.is_empty() => {
                failures.extend(upload_failures);
                let _ = tx.send(TaskMsg::Imported(Ok(articles)));
            }
            UploadResult {
                failures: upload_failures,
                ..
            } => {
                failures.extend(upload_failures);
                let detail = failures
                    .first()
                    .map(|failure| failure.reason.clone())
                    .unwrap_or_else(|| "上传失败".to_string());
                let _ = tx.send(TaskMsg::Imported(Err(anyhow!(detail))));
            }
        }

        if !failures.is_empty() {
            let _ = tx.send(TaskMsg::ImportFailures(failures));
        }
    });
}

fn spawn_create_one(app: &mut RayviewApp, article: NewArticle) {
    let api = app.api.clone();
    app.loading = true;
    app.bus.spawn(move |tx| {
        let result = api.create(&article).map(|article| vec![article]);
        let _ = tx.send(TaskMsg::Imported(result));
    });
}

struct UploadResult {
    articles: Vec<shared::Article>,
    failures: Vec<FailureReport>,
}

fn upload_all(api: &ApiClient, payloads: Vec<NewArticle>) -> UploadResult {
    let mut articles = Vec::new();
    let mut failures = Vec::new();
    for payload in payloads {
        let item = if let Some(pmid) = payload.pmid.as_deref().filter(|pmid| !pmid.is_empty()) {
            format!("PMID {pmid}")
        } else if let Some(doi) = payload.doi.as_deref().filter(|doi| !doi.is_empty()) {
            format!("DOI {doi}")
        } else {
            payload.title.clone()
        };
        match api.create(&payload) {
            Ok(article) => articles.push(article),
            Err(error) => failures.push(FailureReport::new(item, error.to_string())),
        }
    }
    UploadResult { articles, failures }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_reference_inputs() {
        let parsed = parse_reference_input(
            "https://pubmed.ncbi.nlm.nih.gov/12345678/\n\
             PMID:23456789\n\
             https://doi.org/10.1016/j.cell.2020.01.001\n\
             10.1016/j.cell.2020.01.001\n\
             https://www.nature.com/articles/s41586-024-00000-0\n\
             not a reference",
        );

        assert_eq!(parsed.pmids, vec!["12345678", "23456789"]);
        assert_eq!(parsed.article_inputs.len(), 2);
        assert_eq!(parsed.rejected.len(), 1);
    }
}
