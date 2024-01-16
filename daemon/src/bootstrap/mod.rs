use std::sync::Arc;

use tracing::info;

use crate::{
    controllers::{launchpad_mini_mk3::LaunchpadMiniMk3, Alles, Controller},
    scripts::{cover::CoverScript, Script},
};

pub async fn bootstrap(state: Arc<crate::state::AppState>) {
    info!("Starting bootstrap");

    if let Ok(mk3) = LaunchpadMiniMk3::guess() {
        // let mk3 = Arc::new(mk3);
        let mk3: Arc<Box<dyn Alles>> = Arc::new(mk3);
        state.controller_tx.send(mk3.clone()).await.unwrap();

        info!("Added controller");

        let mut script = CoverScript::new();

        let x = tokio::spawn(async move {
            mk3.run(&mut script).await.unwrap();
        });

        state.running_scripts.lock().unwrap().insert("launchpad_mini_mk3_0".to_string(), x);
    }

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
