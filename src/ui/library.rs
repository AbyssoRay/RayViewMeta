use shared::{Article, ArticleSource, ArticleUpdate, Decision};

use crate::app::{RayviewApp, View};
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    egui::Panel::left("library_filters")
        .resizable(true)
        .default_size(270.0)
        .show_inside(root_ui, |ui| {
            theme::page_frame().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        theme::panel_frame().show(ui, |ui| {
                            ui.label(theme::section_label("Filter / Review Queue"));
                            ui.heading("筛选队列");
                            ui.separator();

                            ui.label("搜索标题、摘要、作者");
                            ui.text_edit_singleline(&mut app.filter_text);

                            ui.add_space(10.0);
                            ui.label(theme::section_label("Decision"));
                            ui.horizontal_wrapped(|ui| {
                                let opts = [
                                    (None, "全部"),
                                    (Some(Decision::Undecided), "未决"),
                                    (Some(Decision::Include), "纳入"),
                                    (Some(Decision::Exclude), "排除"),
                                    (Some(Decision::Maybe), "待定"),
                                ];
                                for (val, label) in opts {
                                    let selected = app.filter_decision == val;
                                    if ui.selectable_label(selected, label).clicked() {
                                        app.filter_decision = val;
                                    }
                                }
                            });

                            ui.add_space(10.0);
                            ui.label(theme::section_label("Source"));
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .selectable_label(app.filter_source.is_none(), "全部")
                                    .clicked()
                                {
                                    app.filter_source = None;
                                }
                                for source in [
                                    ArticleSource::Pdf,
                                    ArticleSource::Pubmed,
                                    ArticleSource::Web,
                                    ArticleSource::Manual,
                                ] {
                                    let selected = app.filter_source == Some(source);
                                    if ui
                                        .selectable_label(selected, source_label(source))
                                        .clicked()
                                    {
                                        app.filter_source =
                                            if selected { None } else { Some(source) };
                                    }
                                }
                            });

                            ui.add_space(10.0);
                            ui.checkbox(&mut app.filter_starred, "仅看星标");

                            ui.add_space(10.0);
                            ui.label(theme::section_label("Labels"));
                            let all_tags: std::collections::BTreeSet<String> = app
                                .articles
                                .iter()
                                .flat_map(|a| a.tags.iter().cloned())
                                .chain(app.persisted.custom_tags.iter().cloned())
                                .collect();
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .selectable_label(app.filter_tag.is_none(), "全部")
                                    .clicked()
                                {
                                    app.filter_tag = None;
                                }
                                for tag in &all_tags {
                                    let selected = app.filter_tag.as_deref() == Some(tag.as_str());
                                    if ui.selectable_label(selected, tag).clicked() {
                                        app.filter_tag =
                                            if selected { None } else { Some(tag.clone()) };
                                    }
                                }
                            });

                            ui.add_space(12.0);
                            ui.separator();
                            let stats = stats(&app.articles);
                            ui.label(theme::section_label("Screening Status"));
                            ui.monospace(format!("Total     {:>4}", app.articles.len()));
                            ui.monospace(format!("Include   {:>4}", stats.include));
                            ui.monospace(format!("Exclude   {:>4}", stats.exclude));
                            ui.monospace(format!("Maybe     {:>4}", stats.maybe));
                            ui.monospace(format!("Undecided {:>4}", stats.undecided));
                            ui.monospace(format!("Starred   {:>4}", stats.starred));
                        });
                    });
            });
        });

    egui::CentralPanel::default().show_inside(root_ui, |ui| {
        theme::page_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(theme::section_label("Rayview Meta / Literature Ops"));
                    ui.heading("文献筛选台");
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("导入文献").clicked() {
                        app.view = View::Upload;
                    }
                });
            });
            ui.separator();

            let filtered = app.filtered_article_indices();
            ui.label(
                egui::RichText::new(format!(
                    "当前队列：{} / {}",
                    filtered.len(),
                    app.articles.len()
                ))
                .color(theme::MUTED),
            );
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if filtered.is_empty() {
                        ui.add_space(52.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("没有匹配的文献。调整筛选或导入新文献。")
                                    .color(theme::MUTED),
                            );
                        });
                        return;
                    }

                    let mut to_open: Option<String> = None;
                    let mut to_delete: Option<String> = None;
                    let mut updates: Vec<(String, ArticleUpdate)> = Vec::new();

                    for idx in filtered {
                        let article = &app.articles[idx];
                        let selected = app.selected_id.as_deref() == Some(article.id.as_str());
                        theme::row_frame(selected).show(ui, |ui| {
                            render_row(ui, article, &mut to_open, &mut to_delete, &mut updates);
                        });
                    }

                    if let Some(id) = to_open {
                        app.selected_id = Some(id);
                        app.view = View::Detail;
                    }
                    if let Some(id) = to_delete {
                        app.submit_delete(id);
                    }
                    for (id, update) in updates {
                        app.submit_update(id, update);
                    }
                });
        });
    });
}

fn render_row(
    ui: &mut egui::Ui,
    article: &Article,
    to_open: &mut Option<String>,
    to_delete: &mut Option<String>,
    updates: &mut Vec<(String, ArticleUpdate)>,
) {
    ui.horizontal(|ui| {
        if ui
            .selectable_label(
                article.starred,
                if article.starred { "Starred" } else { "Star" },
            )
            .clicked()
        {
            updates.push((
                article.id.clone(),
                ArticleUpdate {
                    starred: Some(!article.starred),
                    ..Default::default()
                },
            ));
        }
        decision_chip(ui, article.decision);
        ui.label(theme::chip(source_label(article.source), theme::CYAN));
        ui.add_space(4.0);
        let title_response = ui.add(
            egui::Label::new(egui::RichText::new(&article.title).strong().size(19.0))
                .sense(egui::Sense::click())
                .wrap(),
        );
        if title_response.double_clicked() {
            *to_open = Some(article.id.clone());
        }
    });

    ui.horizontal_wrapped(|ui| {
        if let Some(journal) = &article.journal {
            ui.label(egui::RichText::new(journal).italics().color(theme::MUTED));
        }
        if let Some(year) = article.year {
            ui.label(
                egui::RichText::new(format!("/ {year}"))
                    .monospace()
                    .color(theme::MUTED),
            );
        }
        if !article.authors.is_empty() {
            let names = article
                .authors
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if article.authors.len() > 3 {
                " 等"
            } else {
                ""
            };
            ui.label(egui::RichText::new(format!("/ {names}{suffix}")).color(theme::MUTED));
        }
        if let Some(pmid) = &article.pmid {
            ui.label(
                egui::RichText::new(format!("/ PMID {pmid}"))
                    .monospace()
                    .color(theme::MUTED),
            );
        }
    });

    let preview = article
        .abstract_text
        .chars()
        .take(260)
        .collect::<String>()
        .replace('\n', " ");
    if !preview.is_empty() {
        let preview_response = ui.add(
            egui::Label::new(egui::RichText::new(preview).color(theme::TEXT))
                .sense(egui::Sense::click())
                .wrap(),
        );
        if preview_response.double_clicked() {
            *to_open = Some(article.id.clone());
        }
    }

    ui.horizontal_wrapped(|ui| {
        for tag in &article.tags {
            ui.label(theme::chip(format!("#{tag}"), theme::ACCENT));
        }
        if article.decision == Decision::Exclude && !article.exclusion_reason.trim().is_empty() {
            ui.label(theme::chip(
                format!("Reason {}", article.exclusion_reason),
                theme::DANGER,
            ));
        }
    });

    ui.horizontal(|ui| {
        if ui.button("打开详情").clicked() {
            *to_open = Some(article.id.clone());
        }
        for (decision, label) in [
            (Decision::Include, "纳入"),
            (Decision::Exclude, "排除"),
            (Decision::Maybe, "待定"),
            (Decision::Undecided, "未决"),
        ] {
            if ui
                .selectable_label(article.decision == decision, label)
                .clicked()
            {
                updates.push((
                    article.id.clone(),
                    ArticleUpdate {
                        decision: Some(decision),
                        ..Default::default()
                    },
                ));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("删除").clicked() {
                *to_delete = Some(article.id.clone());
            }
        });
    });
}

fn decision_chip(ui: &mut egui::Ui, decision: Decision) {
    let color = match decision {
        Decision::Undecided => theme::LINE,
        Decision::Include => theme::SUCCESS,
        Decision::Exclude => theme::DANGER,
        Decision::Maybe => theme::ACCENT,
    };
    ui.label(theme::chip(decision.label(), color));
}

fn source_label(source: ArticleSource) -> &'static str {
    match source {
        ArticleSource::Manual => "Manual",
        ArticleSource::Pdf => "PDF",
        ArticleSource::Pubmed => "PubMed",
        ArticleSource::Web => "Web",
    }
}

#[derive(Default)]
struct Stats {
    include: usize,
    exclude: usize,
    maybe: usize,
    undecided: usize,
    starred: usize,
}

fn stats(articles: &[Article]) -> Stats {
    let mut stats = Stats::default();
    for article in articles {
        match article.decision {
            Decision::Include => stats.include += 1,
            Decision::Exclude => stats.exclude += 1,
            Decision::Maybe => stats.maybe += 1,
            Decision::Undecided => stats.undecided += 1,
        }
        if article.starred {
            stats.starred += 1;
        }
    }
    stats
}
