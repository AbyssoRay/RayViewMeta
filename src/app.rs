use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shared::{Article, ArticleSource, ArticleUpdate, Project};

use crate::api::{ApiClient, UpdateOutcome, DEFAULT_PROJECT_ID};
use crate::tasks::{
    DeleteManyReport, FailureReport, FontLoadResult, ImportProgress, TaskBus, TaskMsg,
};
use crate::ui;

pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:9631";
pub const DEFAULT_PROJECT_NAME: &str = "通用文献库";
pub const CONFIRM_DELETE_PROJECT: &str = "删除当前文献库";
const FONT_REGULAR_NAME: &str = "source_han_serif_sc";
const FONT_BOLD_NAME: &str = "source_han_serif_sc_bold";
const FONT_REGULAR_FILE: &str = "NotoSerifCJKsc-Regular.otf";
const FONT_BOLD_FILE: &str = "NotoSerifCJKsc-Bold.otf";
const FONT_REGULAR_URL: &str = "https://github.com/notofonts/noto-cjk/raw/main/Serif/OTF/SimplifiedChinese/NotoSerifCJKsc-Regular.otf";
const FONT_BOLD_URL: &str = "https://github.com/notofonts/noto-cjk/raw/main/Serif/OTF/SimplifiedChinese/NotoSerifCJKsc-Bold.otf";
const MAX_CONCURRENT_TRANSLATIONS: usize = 4;

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
    pub error: Option<String>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum View {
    Library,
    Upload,
    Export,
    ProjectManagement,
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
    pub fonts_loading: bool,
    pub import_progress: Option<ImportProgress>,

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
    pub project_rename_name: String,
    pub project_rename_project_id: Option<String>,
    pub confirm_delete_project: String,
    pub last_project_management_sync_at: Option<Instant>,

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
    pub translation_queue: VecDeque<String>,
    pub translation_inflight: BTreeSet<String>,
}

#[derive(Clone, Copy)]
struct FontCacheStatus {
    missing_regular: bool,
    missing_bold: bool,
}

impl FontCacheStatus {
    fn is_ready(self) -> bool {
        !self.missing_regular && !self.missing_bold
    }
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

        let bus = TaskBus::default();

        // 字体只从缓存同步加载；首次联网下载放到后台，避免双击后窗口迟迟不出现。
        let font_cache_status = configure_fonts(&cc.egui_ctx);
        let fonts_ready = font_cache_status.is_ready();
        ui::theme::apply(&cc.egui_ctx);
        let logo_texture = crate::assets::load_logo_texture(&cc.egui_ctx);

        let mut app = Self {
            settings_url_buf: persisted.server_url.clone(),
            persisted,
            api,
            bus,
            view: View::Library,
            projects: Vec::new(),
            articles: Vec::new(),
            selected_id: None,
            status: if fonts_ready {
                "欢迎使用 Rayview Meta".to_string()
            } else {
                "正在准备字体资源".to_string()
            },
            last_status_at: Some(Instant::now()),
            loading: !fonts_ready,
            fonts_loading: !fonts_ready,
            import_progress: None,
            filter_text: String::new(),
            filter_decision: None,
            filter_tag: None,
            filter_source: None,
            filter_starred: false,
            pubmed_input: String::new(),
            manual_title: String::new(),
            manual_abstract: String::new(),
            new_project_name: String::new(),
            project_rename_name: String::new(),
            project_rename_project_id: None,
            confirm_delete_project: String::new(),
            last_project_management_sync_at: None,
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
            translation_queue: VecDeque::new(),
            translation_inflight: BTreeSet::new(),
        };
        if !fonts_ready {
            app.spawn_font_download(font_cache_status);
        }
        app.refresh_projects();
        app
    }

    fn spawn_font_download(&self, cache_status: FontCacheStatus) {
        self.bus.spawn(move |tx| {
            let mut result = FontLoadResult {
                regular: None,
                bold: None,
                errors: Vec::new(),
            };

            let regular = if cache_status.missing_regular {
                download_and_cache_font(FONT_REGULAR_FILE, FONT_REGULAR_URL)
            } else {
                load_cached_font(FONT_REGULAR_FILE)
                    .ok_or_else(|| anyhow::anyhow!("常规字体缓存已不存在"))
            };
            match regular {
                Ok(bytes) => result.regular = Some(bytes),
                Err(error) => result.errors.push(format!("常规字体下载失败: {error}")),
            }

            let bold = if cache_status.missing_bold {
                download_and_cache_font(FONT_BOLD_FILE, FONT_BOLD_URL)
            } else {
                load_cached_font(FONT_BOLD_FILE)
                    .ok_or_else(|| anyhow::anyhow!("粗体字体缓存已不存在"))
            };
            match bold {
                Ok(bytes) => result.bold = Some(bytes),
                Err(error) => result.errors.push(format!("粗体字体下载失败: {error}")),
            }

            let _ = tx.send(TaskMsg::FontsLoaded(result));
        });
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
            .unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string())
    }

    pub fn ensure_project_management_buffer(&mut self) {
        let project_id = self.persisted.selected_project_id.clone();
        if self.project_rename_project_id.as_deref() != Some(project_id.as_str()) {
            self.project_rename_name = self.current_project_name();
            self.project_rename_project_id = Some(project_id);
            self.confirm_delete_project.clear();
        }
    }

    pub fn select_project(&mut self, project_id: String) {
        if self.persisted.selected_project_id == project_id {
            return;
        }
        self.persisted.selected_project_id = project_id.clone();
        self.api.set_project_id(project_id);
        self.selected_id = None;
        self.view = View::Library;
        self.clear_translation_work();
        self.clear_filters();
        self.project_rename_project_id = None;
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

    pub fn submit_rename_current_project(&mut self) {
        let name = self.project_rename_name.trim().to_string();
        if name.is_empty() {
            self.set_status("文献库名称不能为空");
            return;
        }
        let project_id = self.persisted.selected_project_id.clone();
        let api = self.api.clone();
        self.loading = true;
        self.set_status("正在重命名文献库");
        self.bus.spawn(move |tx| {
            let r = api.rename_project(&project_id, &name);
            let _ = tx.send(TaskMsg::ProjectRenamed(r));
        });
    }

    pub fn submit_delete_current_project(&mut self) {
        if self.projects.len() <= 1 {
            self.set_status("至少需要保留一个项目");
            return;
        }
        if self.confirm_delete_project.trim() != CONFIRM_DELETE_PROJECT {
            self.set_status("请输入确认文本后再删除文献库");
            return;
        }
        let project_id = self.persisted.selected_project_id.clone();
        let api = self.api.clone();
        self.loading = true;
        self.set_status("正在删除当前文献库");
        self.confirm_delete_project.clear();
        self.bus.spawn(move |tx| {
            let r = api.delete_project(&project_id).map(|_| project_id);
            let _ = tx.send(TaskMsg::ProjectDeleted(r));
        });
    }

    pub fn ensure_translation_for_article(&mut self, article: &Article) {
        self.queue_translation_for_article(article, true, false);
        self.pump_translation_queue();
    }

    pub fn retry_translation(&mut self, article: &Article) {
        self.translations.remove(&article.id);
        self.queue_translation_for_article(article, true, true);
        self.pump_translation_queue();
    }

    fn clear_translation_work(&mut self) {
        self.translations.clear();
        self.translation_queue.clear();
        self.translation_inflight.clear();
    }

    fn schedule_untranslated_articles(&mut self) -> usize {
        let articles = self.articles.clone();
        let mut queued = 0usize;
        for article in &articles {
            if self.queue_translation_for_article(article, false, false) {
                queued += 1;
            }
        }
        self.pump_translation_queue();
        queued
    }

    fn queue_translation_for_article(
        &mut self,
        article: &Article,
        priority: bool,
        force: bool,
    ) -> bool {
        if !article_needs_translation(article) {
            return false;
        }
        if self.translation_inflight.contains(&article.id) {
            return false;
        }
        if self
            .translations
            .get(&article.id)
            .is_some_and(|record| record.error.is_some() && !force)
        {
            return false;
        }
        if let Some(position) = self
            .translation_queue
            .iter()
            .position(|queued_id| queued_id == &article.id)
        {
            if priority {
                if let Some(id) = self.translation_queue.remove(position) {
                    self.translation_queue.push_front(id);
                }
            }
            return false;
        }
        self.translations.entry(article.id.clone()).or_default();
        if priority {
            self.translation_queue.push_front(article.id.clone());
        } else {
            self.translation_queue.push_back(article.id.clone());
        }
        true
    }

    fn pump_translation_queue(&mut self) {
        while self.translation_inflight.len() < MAX_CONCURRENT_TRANSLATIONS {
            let Some(article_id) = self.translation_queue.pop_front() else {
                break;
            };
            if self.translation_inflight.contains(&article_id) {
                continue;
            }
            let Some(article) = self
                .articles
                .iter()
                .find(|article| article.id == article_id)
                .cloned()
            else {
                self.translations.remove(&article_id);
                continue;
            };
            if !article_needs_translation(&article) {
                self.translations.remove(&article_id);
                continue;
            }
            self.translation_inflight.insert(article_id.clone());
            self.translations.insert(
                article_id.clone(),
                TranslationState {
                    loading: true,
                    error: None,
                },
            );
            let api = self.api.clone();
            let project_id = self.persisted.selected_project_id.clone();
            let keywords = self.persisted.keywords.clone();
            self.bus.spawn(move |tx| {
                let result = translate_and_store(&api, &article, &keywords);
                let _ = tx.send(TaskMsg::TranslationStored {
                    project_id,
                    article_id,
                    result,
                });
            });
        }
    }

    fn upsert_article(&mut self, article: Article) {
        if let Some(slot) = self
            .articles
            .iter_mut()
            .find(|existing| existing.id == article.id)
        {
            *slot = article;
        } else {
            self.articles.push(article);
        }
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
            let keywords = article.keywords.join(" ");
            let haystack = format!(
                "{} {} {} {} {}",
                article.title, article.abstract_text, authors, keywords, article.notes
            )
            .to_lowercase();
            if !haystack.contains(&filter_text) {
                return false;
            }
        }
        true
    }

    pub fn drain_messages(&mut self, ctx: &egui::Context) {
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
                            self.ensure_project_management_buffer();
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
                            self.project_rename_name = project.name.clone();
                            self.project_rename_project_id = Some(project.id.clone());
                            self.projects.push(project);
                            self.selected_id = None;
                            self.articles.clear();
                            self.clear_translation_work();
                            self.clear_filters();
                            self.set_status("项目已创建");
                            self.refresh_projects();
                        }
                        Err(error) => self.set_status(format!("创建项目失败: {error}")),
                    }
                }
                TaskMsg::ProjectRenamed(r) => {
                    self.loading = false;
                    match r {
                        Ok(project) => {
                            if let Some(slot) = self
                                .projects
                                .iter_mut()
                                .find(|existing| existing.id == project.id)
                            {
                                *slot = project.clone();
                            }
                            if self.persisted.selected_project_id == project.id {
                                self.project_rename_name = project.name;
                                self.project_rename_project_id = Some(project.id);
                            }
                            self.set_status("文献库已重命名");
                            self.refresh_projects();
                        }
                        Err(error) => self.set_status(format!("重命名文献库失败: {error}")),
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
                            self.project_rename_project_id = None;
                            self.selected_id = None;
                            self.articles.clear();
                            self.clear_translation_work();
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
                            let queued = self.schedule_untranslated_articles();
                            if queued == 0 {
                                self.set_status(format!("已加载 {} 篇文献", cnt));
                            } else {
                                self.set_status(format!(
                                    "已加载 {cnt} 篇文献，正在后台翻译 {queued} 篇"
                                ));
                            }
                        }
                        Err(e) => self.set_status(format!("加载失败: {e}")),
                    }
                }
                TaskMsg::Imported(r) => {
                    self.loading = false;
                    self.import_progress = None;
                    match r {
                        Ok(added) => {
                            for article in &added {
                                self.upsert_article(article.clone());
                                self.queue_translation_for_article(article, false, false);
                            }
                            self.pump_translation_queue();
                            self.set_status(format!("已导入 {} 篇", added.len()));
                            self.refresh_projects();
                        }
                        Err(e) => self.set_status(format!("导入失败: {e}")),
                    }
                }
                TaskMsg::ImportProgress {
                    project_id,
                    progress,
                } => {
                    if project_id == self.persisted.selected_project_id {
                        self.loading = true;
                        self.set_status(progress.message.clone());
                        self.import_progress = Some(progress);
                    }
                }
                TaskMsg::ImportedOne { project_id, result } => {
                    if project_id != self.persisted.selected_project_id {
                        continue;
                    }
                    match result {
                        Ok(article) => {
                            let article = *article;
                            self.upsert_article(article.clone());
                            self.queue_translation_for_article(&article, false, false);
                            self.pump_translation_queue();
                            self.set_status(format!("已导入：{}", article.title));
                        }
                        Err(error) => self.set_status(format!("单篇导入失败: {error}")),
                    }
                }
                TaskMsg::ImportFinished {
                    project_id,
                    imported,
                    total,
                    failures,
                } => {
                    if project_id != self.persisted.selected_project_id {
                        self.refresh_projects();
                        continue;
                    }
                    self.loading = false;
                    self.import_progress = None;
                    if !failures.is_empty() {
                        let failed = failures.len();
                        self.set_failure_report("导入失败明细", failures);
                        self.set_status(format!(
                            "PDF 导入完成：成功 {imported}/{total} 篇，失败 {failed} 篇"
                        ));
                    } else {
                        self.set_status(format!("PDF 导入完成：成功 {imported}/{total} 篇"));
                    }
                    self.refresh_projects();
                }
                TaskMsg::ImportFailures(items) => {
                    let count = items.len();
                    self.set_failure_report("导入失败明细", items);
                    self.set_status(format!("{count} 条文献未成功导入，已列出原因"));
                }
                TaskMsg::TranslationStored {
                    project_id,
                    article_id,
                    result,
                } => {
                    if project_id != self.persisted.selected_project_id {
                        continue;
                    }
                    self.translation_inflight.remove(&article_id);
                    let record = self.translations.entry(article_id.clone()).or_default();
                    record.loading = false;
                    let mut translation_failed = false;
                    match result {
                        Ok(UpdateOutcome::Applied(article))
                        | Ok(UpdateOutcome::Conflict(article)) => {
                            let article = *article;
                            record.error = None;
                            self.upsert_article(article);
                        }
                        Err(error) => {
                            translation_failed = true;
                            record.error = Some(error.to_string());
                            self.set_status(format!("翻译失败: {error}"));
                        }
                    }
                    self.pump_translation_queue();
                    if !translation_failed {
                        let remaining =
                            self.translation_inflight.len() + self.translation_queue.len();
                        if remaining == 0 {
                            self.set_status("后台翻译已完成");
                        } else {
                            self.set_status(format!("后台翻译中，剩余 {remaining} 篇"));
                        }
                    }
                }
                TaskMsg::Updated(r) => match r {
                    Ok(UpdateOutcome::Applied(article)) => {
                        let article = *article;
                        self.upsert_article(article);
                        self.set_status("已保存");
                    }
                    Ok(UpdateOutcome::Conflict(article)) => {
                        let article = *article;
                        self.upsert_article(article);
                        self.draft_notes_article_id = None;
                        self.set_status("编辑冲突：该字段已被其他客户端修改，已同步服务器当前版本");
                    }
                    Err(e) => self.set_status(format!("保存失败: {e}")),
                },
                TaskMsg::Deleted(r) => match r {
                    Ok(id) => {
                        self.articles.retain(|a| a.id != id);
                        self.translations.remove(&id);
                        self.translation_queue.retain(|queued_id| queued_id != &id);
                        self.translation_inflight.remove(&id);
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
                TaskMsg::FontsLoaded(result) => {
                    self.fonts_loading = false;
                    apply_fonts(ctx, result.regular, result.bold);
                    ui::theme::apply(ctx);
                    if result.errors.is_empty() {
                        self.set_status("字体资源已加载");
                    } else {
                        self.set_status(format!(
                            "字体资源未完全加载，已使用可用字体：{}",
                            result.errors.join("；")
                        ));
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
        for id in &report.deleted_ids {
            self.translations.remove(id);
            self.translation_inflight.remove(id);
        }
        self.translation_queue
            .retain(|queued_id| !report.deleted_ids.iter().any(|id| id == queued_id));
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
            .default_size(egui::vec2(640.0, 360.0))
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

    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        self.drain_messages(&ctx);

        if self.fonts_loading {
            show_startup_loading(root_ui);
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
            return;
        }

        ui::top_bar::show(self, root_ui);
        ui::status_bar::show(self, root_ui);

        match self.view {
            View::Library => ui::library::show(self, root_ui),
            View::Upload => ui::upload::show(self, root_ui),
            View::Export => ui::export_panel::show(self, root_ui),
            View::ProjectManagement => ui::project_management::show(self, root_ui),
            View::Detail => ui::detail::show(self, root_ui),
        }

        self.show_failure_report_window(&ctx);

        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }
}

fn article_needs_translation(article: &Article) -> bool {
    !article.abstract_text.trim().is_empty()
        && article
            .translated_abstract
            .as_deref()
            .is_none_or(|text| text.trim().is_empty())
}

fn translate_and_store(
    api: &ApiClient,
    article: &Article,
    keywords: &[String],
) -> anyhow::Result<UpdateOutcome> {
    let translated = crate::translation::translate_abstract(&article.abstract_text, keywords)?;
    api.update(
        &article.id,
        &ArticleUpdate {
            expected_version: Some(article.version),
            translated_abstract: Some(translated.text),
            translated_keywords: Some(translated.translated_keywords),
            ..Default::default()
        },
    )
}

fn show_startup_loading(root_ui: &mut egui::Ui) {
    egui::CentralPanel::default()
        .frame(ui::theme::page_frame())
        .show_inside(root_ui, |ui| {
            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.add_space(14.0);
                        ui.heading(egui::RichText::new("Rayview Meta").color(ui::theme::TEXT));
                        ui.label(
                            egui::RichText::new("Loading fonts and workspace")
                                .color(ui::theme::MUTED),
                        );
                        ui.add_space(10.0);
                        ui.add(
                            egui::ProgressBar::new(0.35)
                                .animate(true)
                                .desired_width(260.0),
                        );
                    });
                },
            );
        });
}

fn configure_fonts(ctx: &egui::Context) -> FontCacheStatus {
    let regular_font = load_cached_font(FONT_REGULAR_FILE);
    let bold_font = load_cached_font(FONT_BOLD_FILE);
    let status = FontCacheStatus {
        missing_regular: regular_font.is_none(),
        missing_bold: bold_font.is_none(),
    };
    apply_fonts(ctx, regular_font, bold_font);
    status
}

fn apply_fonts(ctx: &egui::Context, regular_font: Option<Vec<u8>>, bold_font: Option<Vec<u8>>) {
    let mut fonts = egui::FontDefinitions::default();
    let existing_proportional = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let existing_monospace = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();

    let has_regular_font = regular_font.is_some();
    let has_bold_font = bold_font.is_some();

    if let Some(bytes) = regular_font {
        fonts.font_data.insert(
            FONT_REGULAR_NAME.to_string(),
            egui::FontData::from_owned(bytes).into(),
        );
    }
    if let Some(bytes) = bold_font {
        fonts.font_data.insert(
            FONT_BOLD_NAME.to_string(),
            egui::FontData::from_owned(bytes).into(),
        );
    }

    let mut title_fonts = Vec::new();
    let mut proportional_fonts = Vec::new();
    let mut monospace_fallbacks = Vec::new();

    if has_bold_font {
        push_unique(&mut title_fonts, FONT_BOLD_NAME.to_string());
    }
    if has_regular_font {
        push_unique(&mut title_fonts, FONT_REGULAR_NAME.to_string());
        push_unique(&mut proportional_fonts, FONT_REGULAR_NAME.to_string());
        push_unique(&mut monospace_fallbacks, FONT_REGULAR_NAME.to_string());
    } else if has_bold_font {
        push_unique(&mut proportional_fonts, FONT_BOLD_NAME.to_string());
        push_unique(&mut monospace_fallbacks, FONT_BOLD_NAME.to_string());
    }

    if let Some(bytes) = load_first_existing(&[
        r"C:\Windows\Fonts\times.ttf",
        r"C:\Windows\Fonts\Times.ttf",
        r"/usr/share/fonts/truetype/msttcorefonts/Times_New_Roman.ttf",
        r"/usr/share/fonts/truetype/msttcorefonts/times.ttf",
        r"/Library/Fonts/Times New Roman.ttf",
    ]) {
        fonts.font_data.insert(
            "times_new_roman".to_string(),
            egui::FontData::from_owned(bytes).into(),
        );
        push_unique(&mut title_fonts, "times_new_roman".to_string());
        push_unique(&mut proportional_fonts, "times_new_roman".to_string());
    }

    for font_name in &existing_proportional {
        push_unique(&mut title_fonts, font_name.clone());
        push_unique(&mut proportional_fonts, font_name.clone());
    }
    for font_name in existing_monospace {
        push_unique(&mut monospace_fallbacks, font_name);
    }

    fonts
        .families
        .insert(egui::FontFamily::Name(FONT_BOLD_NAME.into()), title_fonts);
    fonts
        .families
        .insert(egui::FontFamily::Proportional, proportional_fonts);
    fonts
        .families
        .insert(egui::FontFamily::Monospace, monospace_fallbacks);

    ctx.set_fonts(fonts);
}

fn load_first_existing(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| std::fs::read(path).ok())
}

fn load_cached_font(file_name: &str) -> Option<Vec<u8>> {
    let cache_path = font_cache_dir().map(|dir| dir.join(file_name));
    if let Some(path) = cache_path.as_ref() {
        if let Ok(bytes) = std::fs::read(path) {
            if is_plausible_font(&bytes) {
                return Some(bytes);
            }
        }
    }
    None
}

fn download_and_cache_font(file_name: &str, url: &str) -> anyhow::Result<Vec<u8>> {
    let bytes = download_font(url)?;
    write_cached_font(file_name, &bytes);
    Ok(bytes)
}

fn write_cached_font(file_name: &str, bytes: &[u8]) {
    let Some(path) = font_cache_dir().map(|dir| dir.join(file_name)) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temp_path = path.with_extension("otf.tmp");
    if std::fs::write(&temp_path, bytes).is_ok() {
        let _ = std::fs::rename(temp_path, path);
    }
}

fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.iter().any(|item| item == &value) {
        items.push(value);
    }
}

fn font_cache_dir() -> Option<PathBuf> {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return Some(
            PathBuf::from(local_app_data)
                .join("RayViewMeta")
                .join("fonts"),
        );
    }
    if let Some(cache_home) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(cache_home).join("RayViewMeta").join("fonts"));
    }
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".cache")
            .join("RayViewMeta")
            .join("fonts")
    })
}

fn download_font(url: &str) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("RayviewMeta/0.1 font-loader")
        .build()?;
    let response = client.get(url).send()?.error_for_status()?;
    let bytes = response.bytes()?.to_vec();
    if !is_plausible_font(&bytes) {
        anyhow::bail!("downloaded font file is invalid or too small");
    }
    Ok(bytes)
}

fn is_plausible_font(bytes: &[u8]) -> bool {
    bytes.len() > 1_000_000 && (bytes.starts_with(b"OTTO") || bytes.starts_with(&[0, 1, 0, 0]))
}
