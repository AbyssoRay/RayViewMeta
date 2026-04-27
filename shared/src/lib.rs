use serde::{Deserialize, Serialize};

/// Rayyan 风格的筛选决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    #[default]
    Undecided,
    Include,
    Exclude,
    Maybe,
}

impl Decision {
    pub fn label(&self) -> &'static str {
        match self {
            Decision::Undecided => "未决",
            Decision::Include => "纳入",
            Decision::Exclude => "排除",
            Decision::Maybe => "待定",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub id: String,
    pub title: String,
    pub abstract_text: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub journal: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub doi: Option<String>,
    #[serde(default)]
    pub pmid: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub source: ArticleSource,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub exclusion_reason: String,
    #[serde(default)]
    pub decision: Decision,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub translated_abstract: Option<String>,
    #[serde(default)]
    pub translated_keywords: Vec<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub field_versions: FieldVersions,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldVersions {
    #[serde(default)]
    pub tags: u64,
    #[serde(default)]
    pub starred: u64,
    #[serde(default)]
    pub exclusion_reason: u64,
    #[serde(default)]
    pub decision: u64,
    #[serde(default)]
    pub notes: u64,
    #[serde(default)]
    pub translation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ArticleSource {
    #[default]
    Manual,
    Pdf,
    Pubmed,
    Web,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub article_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProject {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectUpdate {
    pub name: String,
}

/// 上传时使用的负载（无 id / 时间戳，由服务端生成）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewArticle {
    pub title: String,
    pub abstract_text: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub journal: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub doi: Option<String>,
    #[serde(default)]
    pub pmid: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub source: ArticleSource,
}

/// 客户端发送的更新（标签/决定/笔记/翻译）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArticleUpdate {
    /// 客户端发起修改时看到的 Article.version。
    /// 服务端用它进行字段级乐观并发控制。
    pub expected_version: Option<u64>,
    pub tags: Option<Vec<String>>,
    pub starred: Option<bool>,
    pub exclusion_reason: Option<String>,
    pub decision: Option<Decision>,
    pub notes: Option<String>,
    pub translated_abstract: Option<String>,
    pub translated_keywords: Option<Vec<String>>,
}
