use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, broadcast};

use crate::controllers::Controller;

pub struct AppState {
    // pub controllers: Vec<Box<dyn Controller>>,
    pub controller_tx: mpsc::Sender<Arc<Box<dyn Controller>>>,
    pub controllers: Arc<Mutex<Vec<Arc<Box<dyn Controller>>>>>,
}
