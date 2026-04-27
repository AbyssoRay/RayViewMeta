use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use shared::{Article, ArticleUpdate, FieldVersions, NewArticle, Project};

pub const DEFAULT_PROJECT_ID: &str = "default";
pub const DEFAULT_PROJECT_NAME: &str = "通用文献库";

pub enum AddResult {
    Created(Box<Article>),
    Duplicate,
}

pub enum UpdateResult {
    Updated(Article),
    Conflict(Article),
    Missing,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredData {
    #[serde(default)]
    projects: Vec<StoredProject>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredProject {
    id: String,
    name: String,
    #[serde(default)]
    created_at: i64,
    #[serde(default)]
    updated_at: i64,
    #[serde(default)]
    articles: Vec<Article>,
}

struct ProjectLibrary {
    id: String,
    name: String,
    created_at: i64,
    updated_at: i64,
    articles: BTreeMap<String, Article>,
}

/// JSON 文件存储；项目彼此隔离，每个项目都有独立文献库。
pub struct Store {
    path: PathBuf,
    projects: BTreeMap<String, ProjectLibrary>,
}

impl Store {
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let projects = if path.exists() {
            let bytes = std::fs::read(&path)
                .with_context(|| format!("无法读取数据文件 {}", path.display()))?;
            if bytes.is_empty() {
                BTreeMap::new()
            } else {
                load_projects_from_bytes(&bytes, &path)?
            }
        } else {
            BTreeMap::new()
        };
        let mut store = Self { path, projects };
        store.ensure_default_project();
        Ok(store)
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let data = StoredData {
            projects: self
                .projects
                .values()
                .map(|project| StoredProject {
                    id: project.id.clone(),
                    name: project.name.clone(),
                    created_at: project.created_at,
                    updated_at: project.updated_at,
                    articles: project.articles.values().cloned().collect(),
                })
                .collect(),
        };
        let json = serde_json::to_vec_pretty(&data)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn list_projects(&self) -> Vec<Project> {
        let mut projects = self.projects.values().map(project_meta).collect::<Vec<_>>();
        projects.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        projects
    }

    pub fn create_project(&mut self, name: String) -> Result<Project> {
        let name = normalized_project_name("", &name);
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        self.projects.insert(
            id.clone(),
            ProjectLibrary {
                id: id.clone(),
                name,
                created_at: now,
                updated_at: now,
                articles: BTreeMap::new(),
            },
        );
        self.persist()?;
        Ok(self
            .projects
            .get(&id)
            .map(project_meta)
            .expect("project inserted"))
    }

    pub fn rename_project(&mut self, id: &str, name: String) -> Result<Option<Project>> {
        let Some(project) = self.projects.get_mut(id) else {
            return Ok(None);
        };
        let name = normalized_project_name(id, &name);
        project.name = name;
        project.updated_at = chrono::Utc::now().timestamp_millis();
        let project_meta = project_meta(project);
        self.persist()?;
        Ok(Some(project_meta))
    }

    pub fn delete_project(&mut self, id: &str) -> Result<bool> {
        let removed = self.projects.remove(id).is_some();
        if removed {
            self.ensure_default_project();
            self.persist()?;
        }
        Ok(removed)
    }

    pub fn list(&self, project_id: &str) -> Option<Vec<Article>> {
        let project = self.projects.get(project_id)?;
        let mut articles: Vec<Article> = project.articles.values().cloned().collect();
        articles.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Some(articles)
    }

    pub fn get(&self, project_id: &str, id: &str) -> Option<Article> {
        self.projects.get(project_id)?.articles.get(id).cloned()
    }

    pub fn add(&mut self, project_id: &str, new: NewArticle) -> Result<Option<AddResult>> {
        let Some(project) = self.projects.get_mut(project_id) else {
            return Ok(None);
        };
        if find_duplicate(&project.articles, &new) {
            return Ok(Some(AddResult::Duplicate));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let article = Article {
            id: id.clone(),
            title: new.title,
            abstract_text: new.abstract_text,
            authors: new.authors,
            journal: new.journal,
            year: new.year,
            doi: new.doi,
            pmid: new.pmid,
            keywords: new.keywords,
            source: new.source,
            tags: Vec::new(),
            starred: false,
            exclusion_reason: String::new(),
            decision: shared::Decision::Undecided,
            notes: String::new(),
            translated_abstract: None,
            translated_keywords: Vec::new(),
            created_at: now,
            updated_at: now,
            version: 1,
            field_versions: FieldVersions {
                tags: 1,
                starred: 1,
                exclusion_reason: 1,
                decision: 1,
                notes: 1,
                translation: 1,
            },
        };
        project.articles.insert(id, article.clone());
        project.updated_at = now;
        self.persist()?;
        Ok(Some(AddResult::Created(Box::new(article))))
    }

    pub fn update(
        &mut self,
        project_id: &str,
        id: &str,
        upd: ArticleUpdate,
    ) -> Result<UpdateResult> {
        let Some(project) = self.projects.get_mut(project_id) else {
            return Ok(UpdateResult::Missing);
        };
        let Some(article) = project.articles.get_mut(id) else {
            return Ok(UpdateResult::Missing);
        };
        normalize_versions(article);

        if let Some(expected_version) = upd.expected_version {
            if expected_version < article.version
                && has_conflicting_field(article, &upd, expected_version)
            {
                return Ok(UpdateResult::Conflict(article.clone()));
            }
        }

        let next_version = article.version.saturating_add(1).max(1);
        let mut changed = false;
        if let Some(tags) = upd.tags {
            article.tags = tags;
            article.field_versions.tags = next_version;
            changed = true;
        }
        if let Some(starred) = upd.starred {
            article.starred = starred;
            article.field_versions.starred = next_version;
            changed = true;
        }
        if let Some(reason) = upd.exclusion_reason {
            article.exclusion_reason = reason;
            article.field_versions.exclusion_reason = next_version;
            changed = true;
        }
        if let Some(decision) = upd.decision {
            article.decision = decision;
            article.field_versions.decision = next_version;
            changed = true;
        }
        if let Some(notes) = upd.notes {
            article.notes = notes;
            article.field_versions.notes = next_version;
            changed = true;
        }
        if let Some(translated_abstract) = upd.translated_abstract {
            article.translated_abstract = Some(translated_abstract);
            article.field_versions.translation = next_version;
            changed = true;
        }
        if let Some(translated_keywords) = upd.translated_keywords {
            article.translated_keywords = translated_keywords;
            article.field_versions.translation = next_version;
            changed = true;
        }
        if changed {
            article.version = next_version;
            article.updated_at = chrono::Utc::now().timestamp_millis();
            project.updated_at = article.updated_at;
        }
        let cloned = article.clone();
        self.persist()?;
        Ok(UpdateResult::Updated(cloned))
    }

    pub fn delete(&mut self, project_id: &str, id: &str) -> Result<bool> {
        let Some(project) = self.projects.get_mut(project_id) else {
            return Ok(false);
        };
        let removed = project.articles.remove(id).is_some();
        if removed {
            project.updated_at = chrono::Utc::now().timestamp_millis();
            self.persist()?;
        }
        Ok(removed)
    }

    fn ensure_default_project(&mut self) {
        if self.projects.is_empty() {
            let now = chrono::Utc::now().timestamp_millis();
            self.projects.insert(
                DEFAULT_PROJECT_ID.to_string(),
                ProjectLibrary {
                    id: DEFAULT_PROJECT_ID.to_string(),
                    name: DEFAULT_PROJECT_NAME.to_string(),
                    created_at: now,
                    updated_at: now,
                    articles: BTreeMap::new(),
                },
            );
        }
    }
}

fn load_projects_from_bytes(bytes: &[u8], path: &Path) -> Result<BTreeMap<String, ProjectLibrary>> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .with_context(|| format!("解析数据文件失败 {}", path.display()))?;
    let stored_projects = if value.is_array() {
        let articles: Vec<Article> = serde_json::from_value(value)
            .with_context(|| format!("解析旧版数据文件失败 {}", path.display()))?;
        vec![StoredProject {
            id: DEFAULT_PROJECT_ID.to_string(),
            name: DEFAULT_PROJECT_NAME.to_string(),
            created_at: 0,
            updated_at: 0,
            articles,
        }]
    } else {
        serde_json::from_value::<StoredData>(value)
            .with_context(|| format!("解析数据文件失败 {}", path.display()))?
            .projects
    };

    let mut projects = BTreeMap::new();
    for stored in stored_projects {
        let mut articles: BTreeMap<String, Article> = stored
            .articles
            .into_iter()
            .map(|mut article| {
                normalize_versions(&mut article);
                (article.id.clone(), article)
            })
            .collect();
        let created_at = stored.created_at.max(
            articles
                .values()
                .map(|article| article.created_at)
                .min()
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
        );
        let updated_at = stored.updated_at.max(
            articles
                .values()
                .map(|article| article.updated_at)
                .max()
                .unwrap_or(created_at),
        );
        let id = if stored.id.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            stored.id
        };
        let name = normalized_project_name(&id, &stored.name);
        projects.insert(
            id.clone(),
            ProjectLibrary {
                id,
                name,
                created_at,
                updated_at,
                articles: std::mem::take(&mut articles),
            },
        );
    }
    Ok(projects)
}

fn normalized_project_name(id: &str, name: &str) -> String {
    let name = name.trim();
    if id == DEFAULT_PROJECT_ID && (name.is_empty() || name == "Default") {
        DEFAULT_PROJECT_NAME.to_string()
    } else if name.is_empty() {
        "未命名文献库".to_string()
    } else {
        name.to_string()
    }
}

fn project_meta(project: &ProjectLibrary) -> Project {
    Project {
        id: project.id.clone(),
        name: project.name.clone(),
        created_at: project.created_at,
        updated_at: project.updated_at,
        article_count: project.articles.len(),
    }
}

fn find_duplicate(articles: &BTreeMap<String, Article>, new: &NewArticle) -> bool {
    let new_title = normalized_title(&new.title);
    let new_doi = normalized_doi(new.doi.as_deref());
    articles.values().any(|article| {
        let title_matches = !new_title.is_empty() && normalized_title(&article.title) == new_title;
        let doi_matches = new_doi
            .as_ref()
            .zip(normalized_doi(article.doi.as_deref()).as_ref())
            .is_some_and(|(left, right)| left == right);
        title_matches || doi_matches
    })
}

fn normalized_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn normalized_doi(doi: Option<&str>) -> Option<String> {
    doi.map(str::trim).filter(|doi| !doi.is_empty()).map(|doi| {
        let mut value = doi.trim();
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
        value
            .split(['?', '#'])
            .next()
            .unwrap_or(value)
            .trim_end_matches(['.', ',', ';', ':', '，', '。'])
            .to_ascii_lowercase()
    })
}

fn normalize_versions(article: &mut Article) {
    if article.version == 0 {
        article.version = 1;
    }
    if article.updated_at == 0 {
        article.updated_at = article.created_at;
    }
    if article.field_versions.tags == 0 {
        article.field_versions.tags = article.version;
    }
    if article.field_versions.starred == 0 {
        article.field_versions.starred = article.version;
    }
    if article.field_versions.exclusion_reason == 0 {
        article.field_versions.exclusion_reason = article.version;
    }
    if article.field_versions.decision == 0 {
        article.field_versions.decision = article.version;
    }
    if article.field_versions.notes == 0 {
        article.field_versions.notes = article.version;
    }
    if article.field_versions.translation == 0 {
        article.field_versions.translation = article.version;
    }
}

fn has_conflicting_field(article: &Article, update: &ArticleUpdate, expected_version: u64) -> bool {
    (update.tags.is_some() && article.field_versions.tags > expected_version)
        || (update.starred.is_some() && article.field_versions.starred > expected_version)
        || (update.exclusion_reason.is_some()
            && article.field_versions.exclusion_reason > expected_version)
        || (update.decision.is_some() && article.field_versions.decision > expected_version)
        || (update.notes.is_some() && article.field_versions.notes > expected_version)
        || ((update.translated_abstract.is_some() || update.translated_keywords.is_some())
            && article.field_versions.translation > expected_version)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn creates_default_project_with_generic_name() {
        let path = temp_data_path("default_project_name");
        let store = Store::load_or_create(&path).unwrap();
        let projects = store.list_projects();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, DEFAULT_PROJECT_ID);
        assert_eq!(projects[0].name, DEFAULT_PROJECT_NAME);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn renames_project_and_persists() {
        let path = temp_data_path("rename_project");
        let mut store = Store::load_or_create(&path).unwrap();
        let renamed = store
            .rename_project(DEFAULT_PROJECT_ID, "糖尿病综述".to_string())
            .unwrap()
            .unwrap();

        assert_eq!(renamed.name, "糖尿病综述");

        let reloaded = Store::load_or_create(&path).unwrap();
        let projects = reloaded.list_projects();
        assert_eq!(projects[0].name, "糖尿病综述");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn translation_fields_are_persisted_with_articles() {
        let path = temp_data_path("translation_persistence");
        let mut store = Store::load_or_create(&path).unwrap();
        let article = match store
            .add(
                DEFAULT_PROJECT_ID,
                NewArticle {
                    title: "A trial".to_string(),
                    abstract_text: "A source abstract.".to_string(),
                    authors: Vec::new(),
                    journal: None,
                    year: None,
                    doi: Some("10.1000/example".to_string()),
                    pmid: None,
                    keywords: Vec::new(),
                    source: shared::ArticleSource::Manual,
                },
            )
            .unwrap()
            .unwrap()
        {
            AddResult::Created(article) => *article,
            AddResult::Duplicate => panic!("unexpected duplicate"),
        };

        store
            .update(
                DEFAULT_PROJECT_ID,
                &article.id,
                ArticleUpdate {
                    expected_version: Some(article.version),
                    translated_abstract: Some("中文摘要".to_string()),
                    translated_keywords: Some(vec!["关键词".to_string()]),
                    ..Default::default()
                },
            )
            .unwrap();

        let reloaded = Store::load_or_create(&path).unwrap();
        let stored = reloaded.get(DEFAULT_PROJECT_ID, &article.id).unwrap();
        assert_eq!(stored.translated_abstract.as_deref(), Some("中文摘要"));
        assert_eq!(stored.translated_keywords, vec!["关键词"]);

        let _ = fs::remove_file(path);
    }

    fn temp_data_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rayview_meta_{label}_{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
