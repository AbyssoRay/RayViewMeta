use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use crate::api::UpdateOutcome;
use crate::translation::TranslatedAbstract;
use shared::Article;
use shared::Project;

#[derive(Debug, Clone)]
pub struct FailureReport {
    pub item: String,
    pub reason: String,
}

impl FailureReport {
    pub fn new(item: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            item: item.into(),
            reason: reason.into(),
        }
    }
}

#[derive(Debug)]
pub struct DeleteManyReport {
    pub label: String,
    pub requested: usize,
    pub deleted_ids: Vec<String>,
    pub failures: Vec<FailureReport>,
}

/// 异步任务结果。客户端 UI 通过轮询 receiver 获取后台线程的产出。
pub enum TaskMsg {
    ProjectsRefreshed(anyhow::Result<Vec<Project>>),
    ProjectCreated(anyhow::Result<Project>),
    ProjectRenamed(anyhow::Result<Project>),
    ProjectDeleted(anyhow::Result<String>),
    Refreshed(anyhow::Result<Vec<Article>>),
    Imported(anyhow::Result<Vec<Article>>),
    ImportFailures(Vec<FailureReport>),
    Translated {
        article_id: String,
        result: anyhow::Result<TranslatedAbstract>,
    },
    Updated(anyhow::Result<UpdateOutcome>),
    Deleted(anyhow::Result<String>),
    DeletedMany(anyhow::Result<DeleteManyReport>),
}

pub struct TaskBus {
    pub tx: Sender<TaskMsg>,
    pub rx: Receiver<TaskMsg>,
}

impl Default for TaskBus {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx }
    }
}

impl TaskBus {
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce(Sender<TaskMsg>) + Send + 'static,
    {
        let tx = self.tx.clone();
        thread::spawn(move || f(tx));
    }
}
