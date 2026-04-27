use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use shared::{Article, ArticleUpdate, NewArticle, NewProject, Project, ProjectUpdate};

pub const DEFAULT_PROJECT_ID: &str = "default";

pub enum UpdateOutcome {
    Applied(Box<Article>),
    Conflict(Box<Article>),
}

#[derive(Deserialize)]
struct ConflictResponse {
    article: Article,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<String>,
    message: Option<String>,
}

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    project_id: String,
    http: Client,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("build reqwest client");
        Self {
            base_url: base_url.into(),
            project_id: DEFAULT_PROJECT_ID.to_string(),
            http,
        }
    }

    pub fn set_base_url(&mut self, url: String) {
        self.base_url = url;
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn set_project_id(&mut self, project_id: impl Into<String>) {
        self.project_id = project_id.into();
    }

    fn url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{base}{path}")
    }

    fn project_url(&self, path: &str) -> String {
        self.url(&format!("/api/projects/{}{path}", self.project_id))
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let resp = self.http.get(self.url("/api/projects")).send()?;
        let resp = check(resp)?;
        Ok(resp.json()?)
    }

    pub fn create_project(&self, name: &str) -> Result<Project> {
        let payload = NewProject {
            name: name.to_string(),
        };
        let resp = self
            .http
            .post(self.url("/api/projects"))
            .json(&payload)
            .send()?;
        let resp = check(resp)?;
        Ok(resp.json()?)
    }

    pub fn rename_project(&self, id: &str, name: &str) -> Result<Project> {
        let payload = ProjectUpdate {
            name: name.to_string(),
        };
        let resp = self
            .http
            .patch(self.url(&format!("/api/projects/{id}")))
            .json(&payload)
            .send()?;
        let resp = check(resp)?;
        Ok(resp.json()?)
    }

    pub fn delete_project(&self, id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/api/projects/{id}")))
            .send()?;
        check(resp)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Article>> {
        let resp = self.http.get(self.project_url("/articles")).send()?;
        let resp = check(resp)?;
        Ok(resp.json()?)
    }

    pub fn create(&self, n: &NewArticle) -> Result<Article> {
        let resp = self
            .http
            .post(self.project_url("/articles"))
            .json(n)
            .send()?;
        let resp = check(resp)?;
        Ok(resp.json()?)
    }

    pub fn update(&self, id: &str, upd: &ArticleUpdate) -> Result<UpdateOutcome> {
        let resp = self
            .http
            .patch(self.project_url(&format!("/articles/{id}")))
            .json(upd)
            .send()?;
        if resp.status() == StatusCode::CONFLICT {
            let conflict: ConflictResponse = resp.json()?;
            return Ok(UpdateOutcome::Conflict(Box::new(conflict.article)));
        }
        let resp = check(resp)?;
        Ok(UpdateOutcome::Applied(Box::new(resp.json()?)))
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.project_url(&format!("/articles/{id}")))
            .send()?;
        check(resp)?;
        Ok(())
    }
}

fn check(resp: reqwest::blocking::Response) -> Result<reqwest::blocking::Response> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        let body = body.trim();
        if body.is_empty() {
            return Err(anyhow!("HTTP {status}"));
        }
        if let Ok(error) = serde_json::from_str::<ErrorResponse>(body) {
            if let Some(message) = error.error.or(error.message) {
                return Err(anyhow!(message));
            }
        }
        return Err(anyhow!(body.to_string()));
    }
    Ok(resp)
}
