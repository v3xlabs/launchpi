# launchpi

Launchpi is a rust daemon binary that lets you manage your control surfaces.

## Device Support

Although originally built to support the Novation Launchpad, we have redesigned to support:
- Novation Launchpad
- Any midi device
- Any keyboard device
- Elgato Streamdeck
  - Streamdeck
  - Streamdeck Studio

## Mechanisms

Launchpi keeps track of the different "surfaces" that are attached, surfaces can be added via the webui, configuration, or auto discovered via usb if enabled.
Surfaces could range from anything with pure button input (ie a keyboard), to sliders, faders, velocity, and knobs (midi devices), to color feedback (launchpads), and visual feedback (streamdecks).

Panels are a virtual concept that is rendered out to the surface if possible.
A panel could contain icons (for streamdeck like devices), but also plain colors, for launchpads.
Panels have a fixed dimension, that preferablly maps to the desired device, for example some devices are 4x8, 2x16, etc.
Buttons, which are aligned on panels, can have a programatic state, default state, pressed state and more.
Buttons trigger actions when pressed, long pressed, ultra long pressed, released, etc.

## Integrations

Different integrations allow for building cool experiences with buttons. For example, the homeassistant integrations allows for hooking up any light and connecting it.
The http integration allows for making http calls, fetching status via http and basic scripting, and more.

Integrations are delivered as plugins. Each plugin type can be instantiated
more than once, and each instance is configured by its own TOML file. A plugin
exposes actions, which run when a gesture fires, and variables and feedbacks,
which flow back into what a button renders. See `plugins.md` for the design,
`plugin-authoring.md` for adding one, and `configuration.md` for the schemas.

## Goals

Launchpi aims to be fully config based, configuration changes can be made in the webui and saved as config file, and config file should be able to be declarative (via for example nix).
Ease of expansion for future integrations and custom scripting.

## Delivery Roadmap

1. Complete native Elgato network support before expanding the UI framework.
   - Stream Deck Studio: render key state, receive key events, and manage connections from the API.
   - Stream Deck Network Dock: discover the dock, enumerate its attached child Stream Deck, then render and receive input through the child TCP endpoint.
   - Persist managed network surfaces and their panel assignments in configuration.
2. Replace the React frontend with SolidJS after the network surfaces have a stable API.
   - Preserve the surface-management workflow while migrating components incrementally.
    - Add a panel editor and live surface preview driven by the daemon API.
3. Make buttons do something, through the plugin system.
   - Build the plugin engine against an empty registry: instances, variables,
     feedbacks, the action executor, and dependency-tracked re-render.
   - Ship the `http` plugin, the plugin API and the web UI that configures it,
     including action and feedback editors on a button.
   - Build the image pipeline so a key can show artwork rather than only text.
   - Ship the `mpris` plugin, which proves push-driven re-render without polling.
   - Scaffold `hass` and `spotify`.
4. Give panels, devices and plugin instances a copy-TOML button, so a
   declarative user can paste the daemon's own configuration format into Nix.

## Configuration Model

Devices are physical hardware endpoints. Each device declares its layout and
capabilities, and may select one compatible active panel. Panels are reusable
virtual control grids with default and pressed feedback per control. Device
configuration is stored in `devices.toml`; reusable panels are stored in
`panels.toml` and can also be exported independently. Plugin instances are
stored one per file under `plugins/`, where the filename is the instance
identity.

The Stream Deck Studio profile is a horizontal 16-column by 2-row grid. A
Stream Deck XL profile is 4 columns by 8 rows.

## Inspiration

Heavy inspiration and attribution is given to [bitfocus](https://github.com/bitfocus/companion) buttons, not only for sharing an open-source selfhostable solution for control services;
but also for providing most of the inspiration for this project.
