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

## Goals

Launchpi aims to be fully config based, configuration changes can be made in the webui and saved as config file, and config file should be able to be declarative (via for example nix).
Ease of expansion for future integrations and custom scripting.

## Inspiration

Heavy inspiration and attribution is given to [bitfocus](https://github.com/bitfocus/companion) buttons, not only for sharing an open-source selfhostable solution for control services;
but also for providing most of the inspiration for this project.

