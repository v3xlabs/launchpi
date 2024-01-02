use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::controllers::Alles;

pub struct AppState {
    // pub controllers: Vec<Box<dyn Controller>>,
    pub controller_tx: mpsc::Sender<Arc<Box<dyn Alles>>>,
    pub controllers: Arc<Mutex<Vec<Arc<Box<dyn Alles>>>>>,
}
