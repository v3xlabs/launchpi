use midir::{MidiInput, MidiOutput, MidiIO};

mod launchpad_mini_mk1;
mod launchpad_mini_mk3;

pub trait Controller<T> {
    fn from_connection(device: DeviceInfo<Self::DeviceInfo>) -> Result<Self, ()>;

    fn detect_all() -> Result<Vec<dyn DeviceInfo<Self, T>>, ()>;

    // -device specific starts here-

    async fn clear(&self) -> Result<(), ()>;
}

pub struct DeviceInfo {
    name: String,
    // TODO:
    port: String,
}

pub fn list_controllers() -> Result<Vec<dyn DeviceInfo>, ()> {
    let list = vec![];

    list.push_all(launchpad_mini_mk1::detect_all());
    list.push_all(launchpad_mini_mk1::detect_all());
    list.push_all(launchpad_mini_mk1::detect_all());
    list.push_all(launchpad_mini_mk1::detect_all());
    list.push_all(launchpad_mini_mk1::detect_all());

    Ok(list)
}