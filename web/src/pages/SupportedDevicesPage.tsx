import { Link } from "@tanstack/solid-router";
import { Component, For, Show } from "solid-js";

import { DeviceImage } from "../components/DeviceImage";

type Connection = "network" | "dock" | "usb-midi";
type SupportedDevice = {
  model: string;
  grid: string;
  extras: string | null;
  connection: Connection;
  isImplemented: boolean;
};

const connectionLabels: Record<Connection, string> = {
  "network": "Network",
  "dock": "Via dock",
  "usb-midi": "USB MIDI",
};

// Models the daemon knows how to talk to, plus the ones on the way. Grid sizes match
// stream_deck_layout() in daemon/src/streamdeck/studio.rs.
const supportedDevices: SupportedDevice[] = [
  {
    model: "Stream Deck Studio",
    grid: "16 x 2",
    extras: "2 dials",
    connection: "network",
    isImplemented: true,
  },
  {
    model: "Stream Deck Network Dock",
    grid: "Keyless",
    extras: "hosts one Stream Deck",
    connection: "network",
    isImplemented: true,
  },
  { model: "Stream Deck XL", grid: "8 x 4", extras: null, connection: "dock", isImplemented: true },
  { model: "Stream Deck Mk.2", grid: "5 x 3", extras: null, connection: "dock", isImplemented: false },
  { model: "Stream Deck Mini", grid: "3 x 2", extras: null, connection: "dock", isImplemented: false },
  {
    model: "Stream Deck Plus",
    grid: "4 x 2",
    extras: "4 dials, touch strip",
    connection: "dock",
    isImplemented: false,
  },
  { model: "Stream Deck Neo", grid: "4 x 2", extras: "2 touch keys", connection: "dock", isImplemented: false },
  { model: "Stream Deck Pedal", grid: "3 pedals", extras: null, connection: "usb-midi", isImplemented: false },
  { model: "Launchpad X", grid: "8 x 8", extras: null, connection: "usb-midi", isImplemented: false },
  { model: "Launchpad Pro Mk3", grid: "8 x 8", extras: null, connection: "usb-midi", isImplemented: false },
  { model: "Launchpad Mini Mk3", grid: "8 x 8", extras: null, connection: "usb-midi", isImplemented: false },
  { model: "Launchpad Mini Mk1", grid: "8 x 8", extras: null, connection: "usb-midi", isImplemented: false },
];

const DeviceTile: Component<{ device: SupportedDevice; }> = properties => (
  <div class="card flex items-center gap-3 p-3">
    <DeviceImage model={properties.device.model} class="h-12 w-20" />
    <div class="min-w-0 flex-1">
      <p class="row-title">{properties.device.model}</p>
      <p class="row-meta">
        {properties.device.grid}
        <Show when={properties.device.extras}>
          {extras => (
            <>
              {" - "}
              {extras()}
            </>
          )}
        </Show>
      </p>
    </div>
    <div class="flex shrink-0 flex-col items-end gap-1">
      <span class="chip chip-muted">{connectionLabels[properties.device.connection]}</span>
      <Show when={!properties.device.isImplemented}>
        <span class="chip">Planned</span>
      </Show>
    </div>
  </div>
);

export const SupportedDevicesPage: Component = () => (
  <div class="page">
    <div class="page-head">
      <div>
        <p class="breadcrumb">
          <Link to="/devices">Devices</Link>
          <span class="meta-sep">/</span>
          <span class="text-neutral-400">Supported</span>
        </p>
        <h1 class="page-title mt-1">Supported devices</h1>
      </div>
    </div>
    <div class="grid items-start gap-3 sm:grid-cols-2 2xl:grid-cols-3">
      <For each={supportedDevices}>{device => <DeviceTile device={device} />}</For>
    </div>
  </div>
);
