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
                            egui::RichText::new("自动提取标题、摘要和 DOI；可一次选择多个文件。")
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
                        ui.label(theme::section_label("PubMed Batch"));
                        ui.heading("PubMed 批量导入");
                        ui.label(
                            egui::RichText::new(
                                "粘贴 PubMed 链接或 PMID，系统会自动识别并抓取标题与摘要。",
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
                                     https://www.ncbi.nlm.nih.gov/pubmed/34567890",
                                ),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("识别并导入").clicked() {
                                let parsed = crate::pubmed::parse_pubmed_input(&app.pubmed_input);
                                let rejected = parsed
                                    .rejected
                                    .into_iter()
                                    .map(|reason| FailureReport::new("PubMed 输入", reason))
                                    .collect::<Vec<_>>();
                                if parsed.pmids.is_empty() {
                                    let reason = rejected
                                        .first()
                                        .map(|failure| failure.reason.as_str())
                                        .unwrap_or("请输入 PMID 或 PubMed 文献链接");
                                    app.set_status(format!("未识别到可用 PMID：{reason}"));
                                    app.set_failure_report("导入失败明细", rejected);
                                } else {
                                    spawn_pubmed_import(app, parsed.pmids, rejected);
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

fn spawn_pubmed_import(app: &mut RayviewApp, pmids: Vec<String>, rejected: Vec<FailureReport>) {
    let api = app.api.clone();
    app.loading = true;
    let suffix = if rejected.is_empty() {
        String::new()
    } else {
        format!("，已忽略 {} 条无法识别的输入", rejected.len())
    };
    app.set_status(format!("正在抓取 {} 篇 PubMed 文章{suffix}", pmids.len()));
    app.bus.spawn(move |tx| {
        let mut failures = rejected;
        match crate::pubmed::fetch_pubmed_with_failures(&pmids) {
            Ok(fetch) => {
                failures.extend(fetch.failures.into_iter().map(|failure| {
                    FailureReport::new(format!("PMID {}", failure.pmid), failure.reason)
                }));

                if fetch.articles.is_empty() {
                    if !failures.is_empty() {
                        let _ = tx.send(TaskMsg::ImportFailures(failures));
                    }
                    let _ = tx.send(TaskMsg::Imported(Err(anyhow!("未导入任何 PubMed 文献"))));
                    return;
                }

                match upload_all(&api, fetch.articles) {
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
            }
            Err(error) => {
                let reason = error.to_string();
                failures.extend(
                    pmids
                        .into_iter()
                        .map(|pmid| FailureReport::new(format!("PMID {pmid}"), reason.clone())),
                );
                if !failures.is_empty() {
                    let _ = tx.send(TaskMsg::ImportFailures(failures));
                }
                let _ = tx.send(TaskMsg::Imported(Err(anyhow!("PubMed 抓取失败: {reason}"))));
            }
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
