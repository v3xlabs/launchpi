use std::{sync::{Arc, Mutex}, collections::HashMap};

use tokio::{sync::mpsc, task::JoinHandle};

use crate::controllers::Alles;

pub struct AppState {
    // pub controllers: Vec<Box<dyn Controller>>,
    pub controller_tx: mpsc::Sender<Arc<Box<dyn Alles>>>,
    pub controllers: Arc<Mutex<Vec<Arc<Box<dyn Alles>>>>>,

    pub shutdown_tx: mpsc::Sender<()>,

    pub running_scripts: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}
