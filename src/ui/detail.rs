use shared::{ArticleUpdate, Decision};

use crate::app::{RayviewApp, View};
use crate::ui::theme;

pub fn show(app: &mut RayviewApp, root_ui: &mut egui::Ui) {
    let Some(article) = app.selected_article().cloned() else {
        app.view = View::Library;
        return;
    };

    if app.draft_notes_article_id.as_deref() != Some(article.id.as_str()) {
        app.draft_notes_article_id = Some(article.id.clone());
        app.draft_notes_for_article = article.notes.clone();
    }
    app.ensure_translation_for_article(&article);
    let translation = app.translations.get(&article.id).cloned();
    let (previous_id, next_id, queue_position, queue_total) = filtered_navigation(app, &article.id);

    egui::CentralPanel::default().show_inside(root_ui, |ui| {
        theme::page_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("返回队列").clicked() {
                    app.view = View::Library;
                }
                if ui
                    .add_enabled(previous_id.is_some(), egui::Button::new("上一篇"))
                    .clicked()
                {
                    app.selected_id = previous_id.clone();
                    app.draft_notes_article_id = None;
                }
                if ui
                    .add_enabled(next_id.is_some(), egui::Button::new("下一篇"))
                    .clicked()
                {
                    app.selected_id = next_id.clone();
                    app.draft_notes_article_id = None;
                }
                if ui
                    .selectable_label(
                        article.starred,
                        if article.starred { "Starred" } else { "Star" },
                    )
                    .clicked()
                {
                    app.submit_update(
                        article.id.clone(),
                        ArticleUpdate {
                            starred: Some(!article.starred),
                            ..Default::default()
                        },
                    );
                }
                if let Some(position) = queue_position {
                    ui.label(theme::section_label(format!(
                        "Detail / Queue {} of {}",
                        position + 1,
                        queue_total
                    )));
                } else {
                    ui.label(theme::section_label("Detail / Outside Current Filter"));
                }
            });
            ui.add_space(4.0);
            ui.heading(&article.title);
            render_metadata(ui, &article);
            ui.separator();

            egui::Panel::right("detail_right")
                .resizable(true)
                .default_size(360.0)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            theme::panel_frame().show(ui, |ui| {
                                render_decision_panel(app, &article, ui);
                                ui.add_space(14.0);
                                ui.separator();
                                render_tags_panel(app, &article, ui);
                                ui.add_space(14.0);
                                ui.separator();
                                render_notes_panel(app, &article, ui);
                            });
                        });
                });

            theme::panel_frame().show(ui, |ui| {
                ui.label(theme::section_label("Abstract / Keyword Highlight"));
                ui.add_space(4.0);
                render_parallel_abstract(app, &article, translation, ui);
            });
        });
    });
}

fn render_parallel_abstract(
    app: &mut RayviewApp,
    article: &shared::Article,
    translation: Option<crate::app::TranslationState>,
    ui: &mut egui::Ui,
) {
    if article.abstract_text.trim().is_empty() {
        ui.label(egui::RichText::new("该文献暂无摘要。").color(theme::MUTED));
        return;
    }
    ui.columns(2, |columns| {
        columns[0].label(theme::section_label("English Original"));
        columns[0].add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("english_abstract_scroll")
            .auto_shrink([false, false])
            .max_height(520.0)
            .show(&mut columns[0], |ui| {
                render_highlighted(ui, &article.abstract_text, &app.persisted.keywords);
            });

        columns[1].label(theme::section_label("中文翻译"));
        columns[1].add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("chinese_abstract_scroll")
            .auto_shrink([false, false])
            .max_height(520.0)
            .show(&mut columns[1], |ui| match translation.as_ref() {
                Some(record) if record.loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("正在调用联网翻译服务...").color(theme::MUTED),
                        );
                    });
                }
                Some(record) if record.text.is_some() => {
                    let mut highlight_terms = record.translated_keywords.clone();
                    if highlight_terms.is_empty() {
                        highlight_terms = app.persisted.keywords.clone();
                    }
                    render_highlighted(
                        ui,
                        record.text.as_deref().unwrap_or_default(),
                        &highlight_terms,
                    );
                }
                Some(record) if record.error.is_some() => {
                    ui.label(
                        egui::RichText::new(format!(
                            "翻译失败：{}",
                            record.error.as_deref().unwrap_or("未知错误")
                        ))
                        .color(theme::DANGER),
                    );
                    if ui.button("重试翻译").clicked() {
                        app.retry_translation(article);
                    }
                }
                _ => {
                    ui.label(egui::RichText::new("等待翻译任务...").color(theme::MUTED));
                }
            });
    });
}

fn filtered_navigation(
    app: &RayviewApp,
    current_id: &str,
) -> (Option<String>, Option<String>, Option<usize>, usize) {
    let filtered = app.filtered_article_indices();
    let total = filtered.len();
    let Some(position) = filtered
        .iter()
        .position(|index| app.articles[*index].id == current_id)
    else {
        return (None, None, None, total);
    };
    let previous = position
        .checked_sub(1)
        .map(|index| app.articles[filtered[index]].id.clone());
    let next = filtered
        .get(position + 1)
        .map(|index| app.articles[*index].id.clone());
    (previous, next, Some(position), total)
}

fn render_metadata(ui: &mut egui::Ui, article: &shared::Article) {
    let width = ui.available_width();
    theme::panel_frame().show(ui, |ui| {
        ui.set_min_width((width - 32.0).max(320.0));
        ui.label(theme::section_label("Bibliographic Metadata"));
        metadata_row(ui, "作者", |ui| {
            if article.authors.is_empty() {
                ui.label(egui::RichText::new("未提供").color(theme::MUTED));
            } else {
                ui.label(egui::RichText::new(article.authors.join(", ")).color(theme::TEXT));
            }
        });
        metadata_row(ui, "期刊", |ui| {
            if let Some(journal) = &article.journal {
                ui.label(egui::RichText::new(journal).italics().color(theme::TEXT));
            } else {
                ui.label(egui::RichText::new("未提供").color(theme::MUTED));
            }
            if let Some(year) = article.year {
                ui.label(
                    egui::RichText::new(format!("{year}"))
                        .monospace()
                        .color(theme::MUTED),
                );
            }
        });
        metadata_row(ui, "链接", |ui| {
            let mut has_link = false;
            if let Some(pmid) = &article.pmid {
                ui.hyperlink_to(
                    format!("PMID {pmid}"),
                    format!("https://pubmed.ncbi.nlm.nih.gov/{pmid}"),
                );
                has_link = true;
            }
            if let Some(doi) = &article.doi {
                ui.hyperlink_to(format!("DOI {doi}"), format!("https://doi.org/{doi}"));
                has_link = true;
            }
            if !has_link {
                ui.label(egui::RichText::new("未提供").color(theme::MUTED));
            }
        });
        if !article.keywords.is_empty() {
            metadata_row(ui, "关键词", |ui| {
                for keyword in &article.keywords {
                    ui.label(theme::chip(keyword, theme::ACCENT));
                }
            });
        }
    });
}

fn metadata_row(ui: &mut egui::Ui, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.add_sized(
            [56.0, 20.0],
            egui::Label::new(egui::RichText::new(label).strong().color(theme::MUTED)),
        );
        content(ui);
    });
}

fn render_decision_panel(app: &mut RayviewApp, article: &shared::Article, ui: &mut egui::Ui) {
    ui.label(theme::section_label("Decision"));
    let mut new_decision = article.decision;
    ui.horizontal_wrapped(|ui| {
        for (decision, label) in [
            (Decision::Include, "纳入"),
            (Decision::Exclude, "排除"),
            (Decision::Maybe, "待定"),
            (Decision::Undecided, "未决"),
        ] {
            if ui
                .selectable_label(new_decision == decision, label)
                .clicked()
            {
                new_decision = decision;
            }
        }
    });
    if new_decision != article.decision {
        app.submit_update(
            article.id.clone(),
            ArticleUpdate {
                decision: Some(new_decision),
                ..Default::default()
            },
        );
    }

    ui.add_space(8.0);
    ui.label(theme::section_label("Exclusion Reason"));
    let mut reason = article.exclusion_reason.clone();
    ui.horizontal_wrapped(|ui| {
        let presets = app.persisted.exclusion_reasons.clone();
        for preset in presets {
            if ui.selectable_label(reason == preset, &preset).clicked() {
                reason = preset;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut app.draft_reason_for_article);
        if ui.button("设为原因").clicked() {
            let typed = app.draft_reason_for_article.trim().to_string();
            if !typed.is_empty() {
                reason = typed;
                app.draft_reason_for_article.clear();
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button("保存原因").clicked() && reason != article.exclusion_reason {
            app.submit_update(
                article.id.clone(),
                ArticleUpdate {
                    exclusion_reason: Some(reason.clone()),
                    decision: Some(Decision::Exclude),
                    ..Default::default()
                },
            );
        }
        if ui.button("清空原因").clicked() && !article.exclusion_reason.is_empty() {
            app.submit_update(
                article.id.clone(),
                ArticleUpdate {
                    exclusion_reason: Some(String::new()),
                    ..Default::default()
                },
            );
        }
    });
}

fn render_tags_panel(app: &mut RayviewApp, article: &shared::Article, ui: &mut egui::Ui) {
    ui.label(theme::section_label("Labels"));
    let mut tags = article.tags.clone();
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        let mut to_remove: Option<usize> = None;
        for (index, tag) in tags.iter().enumerate() {
            if theme::removable_chip_button(ui, format!("#{tag}"))
                .on_hover_text("单击删除标签")
                .clicked()
            {
                to_remove = Some(index);
            }
        }
        if let Some(index) = to_remove {
            tags.remove(index);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut app.draft_tag_for_article);
        if ui.button("添加").clicked() {
            let tag = app.draft_tag_for_article.trim().to_string();
            if !tag.is_empty() && !tags.contains(&tag) {
                tags.push(tag);
                app.draft_tag_for_article.clear();
                changed = true;
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        let custom_tags = app.persisted.custom_tags.clone();
        for tag in custom_tags {
            if !tags.contains(&tag) && ui.button(format!("+ {tag}")).clicked() {
                tags.push(tag);
                changed = true;
            }
        }
    });
    if changed {
        app.submit_update(
            article.id.clone(),
            ArticleUpdate {
                tags: Some(tags),
                ..Default::default()
            },
        );
    }
}

fn render_notes_panel(app: &mut RayviewApp, article: &shared::Article, ui: &mut egui::Ui) {
    ui.label(theme::section_label("Notes"));
    ui.add(
        egui::TextEdit::multiline(&mut app.draft_notes_for_article)
            .desired_rows(7)
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        if ui.button("保存笔记").clicked() && app.draft_notes_for_article != article.notes {
            app.submit_update(
                article.id.clone(),
                ArticleUpdate {
                    notes: Some(app.draft_notes_for_article.clone()),
                    ..Default::default()
                },
            );
        }
        if ui.button("恢复").clicked() {
            app.draft_notes_for_article = article.notes.clone();
        }
    });
}

fn render_highlighted(ui: &mut egui::Ui, text: &str, keywords: &[String]) {
    let wrap_width = ui.available_width().max(240.0);
    ui.set_min_width(wrap_width);
    if keywords.is_empty() {
        ui.add(egui::Label::new(egui::RichText::new(text).size(19.0).color(theme::TEXT)).wrap());
        return;
    }
    let segments = split_with_keywords(text, keywords);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let normal = egui::TextFormat {
        font_id: egui::FontId::proportional(19.0),
        color: theme::TEXT,
        ..Default::default()
    };
    let highlight = egui::TextFormat {
        font_id: egui::FontId::proportional(19.0),
        color: theme::TEXT,
        background: theme::HIGHLIGHT_BG,
        ..Default::default()
    };
    for (segment, is_highlighted) in segments {
        job.append(
            &segment,
            0.0,
            if is_highlighted {
                highlight.clone()
            } else {
                normal.clone()
            },
        );
    }
    ui.add(egui::Label::new(job).wrap());
}

fn split_with_keywords(text: &str, keywords: &[String]) -> Vec<(String, bool)> {
    let mut patterns: Vec<String> = keywords
        .iter()
        .filter(|keyword| !keyword.trim().is_empty())
        .map(|keyword| regex::escape(keyword.trim()))
        .collect();
    if patterns.is_empty() {
        return vec![(text.to_string(), false)];
    }
    patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.len()));
    let pattern = format!("(?i)({})", patterns.join("|"));
    let Ok(regex) = regex::Regex::new(&pattern) else {
        return vec![(text.to_string(), false)];
    };
    let mut out = Vec::new();
    let mut last = 0usize;
    for matched in regex.find_iter(text) {
        if matched.start() > last {
            out.push((text[last..matched.start()].to_string(), false));
        }
        out.push((text[matched.start()..matched.end()].to_string(), true));
        last = matched.end();
    }
    if last < text.len() {
        out.push((text[last..].to_string(), false));
    }
    out
}
