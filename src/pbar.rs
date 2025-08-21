use std::{cell::RefCell, sync::Arc};

use crate::utils::yield_async;

pub struct PBar {
    progress: f32,
    status: String,
}

pub trait Progress: Clone {
    async fn update_progress(&self, progress: f32);

    async fn update_status(&self, status: String);

    fn update_progress_sync(&self, progress: f32);

    fn update_status_sync(&self, status: String);

    fn get_progress(&self) -> f32;

    fn get_status(&self) -> String;
}

pub type ProgressBar = Arc<RefCell<PBar>>;

impl Progress for ProgressBar {
    async fn update_progress(&self, progress: f32) {
        self.borrow_mut().progress = progress;
        yield_async(0).await;
    }

    async fn update_status(&self, status: String) {
        self.borrow_mut().status = status;
        yield_async(0).await;
    }

    fn update_progress_sync(&self, progress: f32) {
        self.borrow_mut().progress = progress;
    }

    fn update_status_sync(&self, status: String) {
        self.borrow_mut().status = status;
    }

    fn get_progress(&self) -> f32 {
        self.borrow().progress
    }

    fn get_status(&self) -> String {
        self.borrow().status.clone()
    }
}

pub fn make_progress_bar() -> ProgressBar {
    Arc::new(RefCell::new(PBar {
        progress: 0.0,
        status: "".to_string(),
    }))
}
