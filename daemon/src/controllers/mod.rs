use launchy::MidiError;
use serde::{Deserialize, Serialize};

use crate::scripts::Script;

pub mod launchpad_mini_mk1;
pub mod launchpad_mini_mk3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ControllerEvent {
    Heartbeat,
    Press { x: u8, y: u8 },
    Release { x: u8, y: u8 },
}

#[async_trait::async_trait]
pub trait Controller: Send + Sync {
    fn guess() -> Result<Box<Self>, MidiError>
    where
        Self: Sized;

    fn guess_ok() -> Result<(), MidiError>
    where
        Self: Sized;

    // -device specific starts here-

    fn initialize(&self) -> Result<(), MidiError> {
        Ok(())
    }

    fn clear(&self) -> Result<(), MidiError>;

    fn set_button_color(&self, x: u8, y: u8, color: u8) -> Result<(), MidiError>;

    fn get_event_receiver(&self) -> Result<tokio::sync::broadcast::Receiver<ControllerEvent>, ()> {
        todo!("Implement get_event_receiver")
    }

    fn name(&self) -> &str;
}

#[async_trait::async_trait]
pub trait ScriptRunner: Send {
    async fn run(&self, script: &mut dyn Script) -> Result<(), MidiError>;
}

#[async_trait::async_trait]
pub trait Alles: Controller + ScriptRunner + Send + Sync {}
