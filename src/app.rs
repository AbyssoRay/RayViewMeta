use std::collections::BTreeMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use shared::{Article, ArticleSource, ArticleUpdate, Project};

use crate::api::{ApiClient, UpdateOutcome, DEFAULT_PROJECT_ID};
use crate::tasks::{DeleteManyReport, FailureReport, TaskBus, TaskMsg};
use crate::ui;

pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:9631";

#[derive(Serialize, Deserialize)]
pub struct PersistedState {
    pub server_url: String,
    pub keywords: Vec<String>,
    pub custom_tags: Vec<String>,
    #[serde(default)]
    pub exclusion_reasons: Vec<String>,
    #[serde(default = "default_project_id")]
    pub selected_project_id: String,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            server_url: DEFAULT_SERVER_URL.to_string(),
            keywords: vec![],
            custom_tags: vec![
                "RCT".into(),
                "Meta-analysis".into(),
                "Review".into(),
                "Animal".into(),
                "Pediatric".into(),
            ],
            exclusion_reasons: vec![
                "Wrong population".into(),
                "Wrong intervention".into(),
                "Wrong outcome".into(),
                "Wrong study design".into(),
                "Not original research".into(),
            ],
            selected_project_id: DEFAULT_PROJECT_ID.to_string(),
        }
    }
}

fn default_project_id() -> String {
    DEFAULT_PROJECT_ID.to_string()
}

#[derive(Clone, Default)]
pub struct TranslationState {
    pub loading: bool,
    pub text: Option<String>,
    pub translated_keywords: Vec<String>,
    pub error: Option<String>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum View {
    Library,
    Upload,
    Export,
    Settings,
    Detail,
}

pub struct RayviewApp {
    pub persisted: PersistedState,
    pub api: ApiClient,
    pub bus: TaskBus,
    pub view: View,
    pub projects: Vec<Project>,
    pub articles: Vec<Article>,
    pub selected_id: Option<String>,
    pub status: String,
    pub last_status_at: Option<Instant>,
    pub loading: bool,

    // 过滤
    pub filter_text: String,
    pub filter_decision: Option<shared::Decision>,
    pub filter_tag: Option<String>,
    pub filter_source: Option<ArticleSource>,
    pub filter_starred: bool,

    // 上传
    pub pubmed_input: String,
    pub manual_title: String,
    pub manual_abstract: String,

    // 项目管理
    pub new_project_name: String,

    // 导入失败 / 批量操作结果弹窗
    pub failure_report_title: String,
    pub failure_report_items: Vec<FailureReport>,
    pub show_failure_report: bool,

    // 导出页二次确认输入
    pub confirm_delete_all: String,
    pub confirm_delete_not_included: String,
    pub confirm_clear_filters: String,

    // 关键字 / 标签编辑临时缓冲
    pub new_keyword: String,
    pub new_tag: String,
    pub new_exclusion_reason: String,
    pub draft_tag_for_article: String,
    pub draft_reason_for_article: String,
    pub draft_notes_article_id: Option<String>,
    pub draft_notes_for_article: String,

    // 设置
    pub settings_url_buf: String,

    // 图片 / 翻译缓存
    pub logo_texture: Option<egui::TextureHandle>,
    pub translations: BTreeMap<String, TranslationState>,
}

impl RayviewApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 加载持久化数据
        let mut persisted: PersistedState = cc
            .storage
            .and_then(|s| eframe::get_value::<PersistedState>(s, eframe::APP_KEY))
            .unwrap_or_default();
        if let Ok(server_url) = std::env::var("RAYVIEW_SERVER_URL") {
            if !server_url.trim().is_empty() {
                persisted.server_url = server_url;
            }
        }
        let mut api = ApiClient::new(persisted.server_url.clone());
        api.set_project_id(persisted.selected_project_id.clone());

        // 中文字体
        configure_fonts(&cc.egui_ctx);
        ui::theme::apply(&cc.egui_ctx);
        let logo_texture = crate::assets::load_logo_texture(&cc.egui_ctx);

        let bus = TaskBus::default();
        let mut app = Self {
            settings_url_buf: persisted.server_url.clone(),
            persisted,
            api,
            bus,
            view: View::Library,
            projects: Vec::new(),
            articles: Vec::new(),
            selected_id: None,
            status: "欢迎使用 Rayview Meta".to_string(),
            last_status_at: Some(Instant::now()),
            loading: false,
            filter_text: String::new(),
            filter_decision: None,
            filter_tag: None,
            filter_source: None,
            filter_starred: false,
            pubmed_input: String::new(),
            manual_title: String::new(),
            manual_abstract: String::new(),
            new_project_name: String::new(),
            failure_report_title: String::new(),
            failure_report_items: Vec::new(),
            show_failure_report: false,
            confirm_delete_all: String::new(),
            confirm_delete_not_included: String::new(),
            confirm_clear_filters: String::new(),
            new_keyword: String::new(),
            new_tag: String::new(),
            new_exclusion_reason: String::new(),
            draft_tag_for_article: String::new(),
            draft_reason_for_article: String::new(),
            draft_notes_article_id: None,
            draft_notes_for_article: String::new(),
            logo_texture,
            translations: BTreeMap::new(),
        };
        app.refresh_projects();
        app
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.last_status_at = Some(Instant::now());
    }

    pub fn refresh_projects(&mut self) {
        let api = self.api.clone();
        self.loading = true;
        self.bus.spawn(move |tx| {
            let r = api.list_projects();
            let _ = tx.send(TaskMsg::ProjectsRefreshed(r));
        });
    }

    pub fn refresh(&mut self) {
        let api = self.api.clone();
        self.loading = true;
        self.bus.spawn(move |tx| {
            let r = api.list();
            let _ = tx.send(TaskMsg::Refreshed(r));
        });
    }

    pub fn current_project_name(&self) -> String {
        self.projects
            .iter()
            .find(|project| project.id == self.persisted.selected_project_id)
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "Default".to_string())
    }

    pub fn select_project(&mut self, project_id: String) {
        if self.persisted.selected_project_id == project_id {
            return;
        }
        self.persisted.selected_project_id = project_id.clone();
        self.api.set_project_id(project_id);
        self.selected_id = None;
        self.view = View::Library;
        self.translations.clear();
        self.clear_filters();
        self.refresh();
    }

    pub fn submit_create_project(&mut self) {
        let name = self.new_project_name.trim().to_string();
        if name.is_empty() {
            self.set_status("项目名称不能为空");
            return;
        }
        self.new_project_name.clear();
        let api = self.api.clone();
        self.loading = true;
        self.set_status("正在新建项目");
        self.bus.spawn(move |tx| {
            let r = api.create_project(&name);
            let _ = tx.send(TaskMsg::ProjectCreated(r));
        });
    }

    pub fn submit_delete_current_project(&mut self) {
        if self.projects.len() <= 1 {
            self.set_status("至少需要保留一个项目");
            return;
        }
        let project_id = self.persisted.selected_project_id.clone();
        let api = self.api.clone();
        self.loading = true;
        self.set_status("正在删除当前项目");
        self.bus.spawn(move |tx| {
            let r = api.delete_project(&project_id).map(|_| project_id);
            let _ = tx.send(TaskMsg::ProjectDeleted(r));
        });
    }

    pub fn ensure_translation_for_article(&mut self, article: &Article) {
        if article.abstract_text.trim().is_empty() {
            return;
        }
        if self
            .translations
            .get(&article.id)
            .is_some_and(|record| record.loading || record.text.is_some() || record.error.is_some())
        {
            return;
        }
        self.translations.insert(
            article.id.clone(),
            TranslationState {
                loading: true,
                ..Default::default()
            },
        );
        let article_id = article.id.clone();
        let abstract_text = article.abstract_text.clone();
        let keywords = self.persisted.keywords.clone();
        self.bus.spawn(move |tx| {
            let result = crate::translation::translate_abstract(&abstract_text, &keywords);
            let _ = tx.send(TaskMsg::Translated { article_id, result });
        });
    }

    pub fn retry_translation(&mut self, article: &Article) {
        self.translations.remove(&article.id);
        self.ensure_translation_for_article(article);
    }

    pub fn save_persistent(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.persisted);
    }

    pub fn selected_article(&self) -> Option<&Article> {
        let id = self.selected_id.as_ref()?;
        self.articles.iter().find(|a| &a.id == id)
    }

    pub fn set_failure_report(&mut self, title: impl Into<String>, items: Vec<FailureReport>) {
        if items.is_empty() {
            return;
        }
        self.failure_report_title = title.into();
        self.failure_report_items = items;
        self.show_failure_report = true;
    }

    pub fn clear_filters(&mut self) {
        self.filter_text.clear();
        self.filter_decision = None;
        self.filter_tag = None;
        self.filter_source = None;
        self.filter_starred = false;
    }

    pub fn filtered_article_indices(&self) -> Vec<usize> {
        self.articles
            .iter()
            .enumerate()
            .filter(|(_, article)| self.article_matches_filters(article))
            .map(|(index, _)| index)
            .collect()
    }

    fn article_matches_filters(&self, article: &Article) -> bool {
        if let Some(decision) = self.filter_decision {
            if article.decision != decision {
                return false;
            }
        }
        if let Some(source) = self.filter_source {
            if article.source != source {
                return false;
            }
        }
        if self.filter_starred && !article.starred {
            return false;
        }
        if let Some(tag) = &self.filter_tag {
            if !article.tags.iter().any(|item| item == tag) {
                return false;
            }
        }
        let filter_text = self.filter_text.trim().to_lowercase();
        if !filter_text.is_empty() {
            let authors = article.authors.join(" ");
            let haystack = format!(
                "{} {} {} {}",
                article.title, article.abstract_text, authors, article.notes
            )
            .to_lowercase();
            if !haystack.contains(&filter_text) {
                return false;
            }
        }
        true
    }

    pub fn drain_messages(&mut self) {
        while let Ok(msg) = self.bus.rx.try_recv() {
            match msg {
                TaskMsg::ProjectsRefreshed(r) => {
                    self.loading = false;
                    match r {
                        Ok(projects) => {
                            self.projects = projects;
                            if !self
                                .projects
                                .iter()
                                .any(|project| project.id == self.persisted.selected_project_id)
                            {
                                if let Some(project) = self.projects.first() {
                                    self.persisted.selected_project_id = project.id.clone();
                                }
                            }
                            self.api
                                .set_project_id(self.persisted.selected_project_id.clone());
                            self.refresh();
                        }
                        Err(error) => self.set_status(format!("加载项目失败: {error}")),
                    }
                }
                TaskMsg::ProjectCreated(r) => {
                    self.loading = false;
                    match r {
                        Ok(project) => {
                            self.persisted.selected_project_id = project.id.clone();
                            self.api.set_project_id(project.id.clone());
                            self.projects.push(project);
                            self.selected_id = None;
                            self.articles.clear();
                            self.translations.clear();
                            self.clear_filters();
                            self.set_status("项目已创建");
                            self.refresh_projects();
                        }
                        Err(error) => self.set_status(format!("创建项目失败: {error}")),
                    }
                }
                TaskMsg::ProjectDeleted(r) => {
                    self.loading = false;
                    match r {
                        Ok(deleted_id) => {
                            self.projects.retain(|project| project.id != deleted_id);
                            if self.persisted.selected_project_id == deleted_id {
                                self.persisted.selected_project_id = self
                                    .projects
                                    .first()
                                    .map(|project| project.id.clone())
                                    .unwrap_or_else(default_project_id);
                                self.api
                                    .set_project_id(self.persisted.selected_project_id.clone());
                            }
                            self.selected_id = None;
                            self.articles.clear();
                            self.translations.clear();
                            self.clear_filters();
                            self.set_status("项目已删除");
                            self.refresh_projects();
                        }
                        Err(error) => self.set_status(format!("删除项目失败: {error}")),
                    }
                }
                TaskMsg::Refreshed(r) => {
                    self.loading = false;
                    match r {
                        Ok(list) => {
                            let cnt = list.len();
                            self.articles = list;
                            self.set_status(format!("已加载 {} 篇文献", cnt));
                        }
                        Err(e) => self.set_status(format!("加载失败: {e}")),
                    }
                }
                TaskMsg::Imported(r) => {
                    self.loading = false;
                    match r {
                        Ok(added) => {
                            self.set_status(format!("已导入 {} 篇", added.len()));
                            self.refresh_projects();
                        }
                        Err(e) => self.set_status(format!("导入失败: {e}")),
                    }
                }
                TaskMsg::ImportFailures(items) => {
                    let count = items.len();
                    self.set_failure_report("导入失败明细", items);
                    self.set_status(format!("{count} 条文献未成功导入，已列出原因"));
                }
                TaskMsg::Translated { article_id, result } => {
                    let record = self.translations.entry(article_id).or_default();
                    record.loading = false;
                    match result {
                        Ok(translated) => {
                            record.text = Some(translated.text);
                            record.translated_keywords = translated.translated_keywords;
                            record.error = None;
                        }
                        Err(error) => {
                            record.error = Some(error.to_string());
                            record.text = None;
                            record.translated_keywords.clear();
                        }
                    }
                }
                TaskMsg::Updated(r) => match r {
                    Ok(UpdateOutcome::Applied(article)) => {
                        let article = *article;
                        if let Some(slot) = self.articles.iter_mut().find(|x| x.id == article.id) {
                            *slot = article;
                        }
                        self.set_status("已保存");
                    }
                    Ok(UpdateOutcome::Conflict(article)) => {
                        let article = *article;
                        if let Some(slot) = self.articles.iter_mut().find(|x| x.id == article.id) {
                            *slot = article;
                        }
                        self.draft_notes_article_id = None;
                        self.set_status("编辑冲突：该字段已被其他客户端修改，已同步服务器当前版本");
                    }
                    Err(e) => self.set_status(format!("保存失败: {e}")),
                },
                TaskMsg::Deleted(r) => match r {
                    Ok(id) => {
                        self.articles.retain(|a| a.id != id);
                        if self.selected_id.as_deref() == Some(&id) {
                            self.selected_id = None;
                            self.view = View::Library;
                        }
                        self.set_status("已删除");
                        self.refresh_projects();
                    }
                    Err(e) => self.set_status(format!("删除失败: {e}")),
                },
                TaskMsg::DeletedMany(r) => {
                    self.loading = false;
                    match r {
                        Ok(report) => self.apply_delete_many_report(report),
                        Err(e) => self.set_status(format!("批量删除失败: {e}")),
                    }
                }
            }
        }
    }

    pub fn submit_update(&mut self, id: String, mut upd: ArticleUpdate) {
        if upd.expected_version.is_none() {
            upd.expected_version = self
                .articles
                .iter()
                .find(|article| article.id == id)
                .map(|article| article.version);
        }
        let api = self.api.clone();
        self.bus.spawn(move |tx| {
            let r = api.update(&id, &upd);
            let _ = tx.send(TaskMsg::Updated(r));
        });
    }

    pub fn submit_delete(&mut self, id: String) {
        let api = self.api.clone();
        self.bus.spawn(move |tx| {
            let r = api.delete(&id).map(|_| id);
            let _ = tx.send(TaskMsg::Deleted(r));
        });
    }

    pub fn submit_delete_many(&mut self, targets: Vec<(String, String)>, label: impl Into<String>) {
        if targets.is_empty() {
            self.set_status("没有符合条件的文献需要删除");
            return;
        }
        let api = self.api.clone();
        let label = label.into();
        let requested = targets.len();
        self.loading = true;
        self.set_status(format!("正在执行批量删除：{label}"));
        self.bus.spawn(move |tx| {
            let mut deleted_ids = Vec::new();
            let mut failures = Vec::new();
            for (id, title) in targets {
                match api.delete(&id) {
                    Ok(()) => deleted_ids.push(id),
                    Err(error) => failures.push(FailureReport::new(title, error.to_string())),
                }
            }
            let _ = tx.send(TaskMsg::DeletedMany(Ok(DeleteManyReport {
                label,
                requested,
                deleted_ids,
                failures,
            })));
        });
    }

    fn apply_delete_many_report(&mut self, report: DeleteManyReport) {
        let deleted_count = report.deleted_ids.len();
        self.articles
            .retain(|article| !report.deleted_ids.iter().any(|id| id == &article.id));
        if self
            .selected_id
            .as_ref()
            .is_some_and(|selected| report.deleted_ids.iter().any(|id| id == selected))
        {
            self.selected_id = None;
            self.view = View::Library;
        }
        if report.failures.is_empty() {
            self.set_status(format!(
                "{}：已删除 {deleted_count} / {} 篇文献",
                report.label, report.requested
            ));
        } else {
            let failure_count = report.failures.len();
            self.set_failure_report(format!("{}失败明细", report.label), report.failures);
            self.set_status(format!(
                "{}：已删除 {deleted_count} / {} 篇，{failure_count} 篇失败",
                report.label, report.requested
            ));
        }
        self.refresh_projects();
    }

    fn show_failure_report_window(&mut self, ctx: &egui::Context) {
        if !self.show_failure_report {
            return;
        }
        let mut open = true;
        egui::Window::new(self.failure_report_title.clone())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(640.0)
            .default_height(360.0)
            .show(ctx, |ui| {
                ui.label(format!(
                    "共 {} 条记录未成功处理。",
                    self.failure_report_items.len()
                ));
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for item in &self.failure_report_items {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new(&item.item).strong());
                                ui.label(&item.reason);
                            });
                        }
                    });
                ui.separator();
                if ui.button("关闭").clicked() {
                    self.show_failure_report = false;
                }
            });
        if !open {
            self.show_failure_report = false;
        }
    }
}

impl eframe::App for RayviewApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_persistent(storage);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();

        ui::top_bar::show(self, ctx);
        ui::status_bar::show(self, ctx);

        match self.view {
            View::Library => ui::library::show(self, ctx),
            View::Upload => ui::upload::show(self, ctx),
            View::Export => ui::export_panel::show(self, ctx),
            View::Settings => ui::settings::show(self, ctx),
            View::Detail => ui::detail::show(self, ctx),
        }

        self.show_failure_report_window(ctx);

        // 持续重绘以便后台任务即时反映
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut proportional_fonts = Vec::new();
    let mut fallback_fonts = Vec::new();

    if let Some(bytes) = load_first_existing(&[
        r"C:\Windows\Fonts\times.ttf",
        r"C:\Windows\Fonts\Times.ttf",
        r"/usr/share/fonts/truetype/msttcorefonts/Times_New_Roman.ttf",
        r"/usr/share/fonts/truetype/msttcorefonts/times.ttf",
        r"/Library/Fonts/Times New Roman.ttf",
    ]) {
        fonts.font_data.insert(
            "times_new_roman".to_string(),
            egui::FontData::from_owned(bytes),
        );
        proportional_fonts.push("times_new_roman".to_string());
    }

    if let Some(bytes) = load_first_existing(&[
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\msyhbd.ttc",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        r"/usr/share/fonts/opentype/source-han-sans/SourceHanSansSC-Regular.otf",
        r"/System/Library/Fonts/PingFang.ttc",
    ]) {
        fonts
            .font_data
            .insert("cjk_ui".to_string(), egui::FontData::from_owned(bytes));
        fallback_fonts.push("cjk_ui".to_string());
    }

    if let Some(bytes) = load_first_existing(&[
        r"C:\Windows\Fonts\simsun.ttc",
        r"C:\Windows\Fonts\simsun.ttf",
        r"C:\Windows\Fonts\NSimSun.ttf",
        r"/usr/share/fonts/opentype/source-han-serif/SourceHanSerifSC-Regular.otf",
        r"/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
        r"/System/Library/Fonts/Supplemental/Songti.ttc",
    ]) {
        fonts
            .font_data
            .insert("songti".to_string(), egui::FontData::from_owned(bytes));
        fallback_fonts.push("songti".to_string());
    }

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let existing = fonts.families.get(&family).cloned().unwrap_or_default();
        let mut merged = proportional_fonts.clone();
        merged.extend(fallback_fonts.clone());
        for font_name in existing {
            if !merged.contains(&font_name) {
                merged.push(font_name);
            }
        }
        fonts.families.insert(family, merged);
    }
    ctx.set_fonts(fonts);
}

fn load_first_existing(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| std::fs::read(path).ok())
}
