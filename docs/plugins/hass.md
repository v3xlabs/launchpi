.
# Home Assistant

## Preset roadmap

The Home Assistant plugin should offer presets for useful controls and readings without flooding the picker with every entity in an installation.

### Readings

- Temperature, humidity, air quality, pressure, illuminance, and power readings.
- Battery percentage with a bar and a low-battery warning border.
- Door, window, motion, occupancy, moisture, smoke, and connectivity status.
- Weather condition, temperature, and precipitation.
- Energy consumption, solar generation, and grid import or export.

Only curated sensor and binary-sensor device classes should become presets. All other readings remain available through the value picker.

### Controls

- Covers and garage doors with separate Open, Close, and Stop controls.
- Locks with separate one-second-hold Lock and Unlock controls.
- Climate controls for current temperature, target temperature, and HVAC mode.
- Vacuum controls for start, pause, return to dock, and state.
- Timer controls for remaining time, start, cancel, and finish.
- Camera snapshots where an entity exposes an image URL.

### Catalogue metadata

Preset generation needs more than an entity ID, friendly name, domain, and icon. Capture device class, unit of measurement, and selected attributes so sensors can be classified and rendered correctly.

### Safety

Do not create one-press defaults for locks, alarms, updates, or destructive automations. Use explicit actions and require a hold gesture or confirmation for those controls.
