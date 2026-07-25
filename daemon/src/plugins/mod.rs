pub mod config;
pub mod engine;
pub mod feedback;
pub mod index;
pub mod instance;
pub mod manifest;
pub mod plugin;
pub mod render;
pub mod secret;
pub mod variables;

use crate::plugins::plugin::PluginFactory;

/// Every plugin type compiled into this daemon.
///
/// Adding one is a module and an entry here. There is no registration macro, and no dynamic
/// loading: a plugin that needs D-Bus depends on a D-Bus crate.
pub fn registry() -> &'static [PluginFactory] {
    &[]
}
