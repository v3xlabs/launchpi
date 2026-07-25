pub mod builtin;
pub mod engine;
pub mod instance;
pub mod manifest;
pub mod plugin;

use crate::plugins::plugin::PluginFactory;

/// Every plugin type compiled into this daemon.
///
/// Adding one is a module and an entry here. There is no registration macro, and no dynamic
/// loading: a plugin that needs D-Bus depends on a D-Bus crate.
pub fn registry() -> &'static [PluginFactory] {
    &[
        builtin::http::FACTORY,
        builtin::mpris::FACTORY,
        builtin::hass::FACTORY,
    ]
}
