use std::sync::Arc;

use tokio::sync::RwLock;

use crate::store::Store;

pub struct AppState {
    pub store: RwLock<Store>,
}

impl AppState {
    pub fn new(store: Store) -> Self {
        Self {
            store: RwLock::new(store),
        }
    }
}

pub type SharedState = Arc<AppState>;
