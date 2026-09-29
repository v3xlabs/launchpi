import * as Dialog from "@kobalte/core/dialog";
import { FiInfo, FiX } from "solid-icons/fi";
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

// Models the daemon knows how to talk to, plus the ones on the way. Grids and dials match the model
// table in daemon/src/drivers/streamdeck/model.rs, which also covers the ones missing here.
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

export const SupportedDevicesDialog: Component = () => (
  <Dialog.Root>
    <Dialog.Trigger class="icon-button" aria-label="Supported devices" title="Supported devices">
      <FiInfo class="size-4" />
    </Dialog.Trigger>
    <Dialog.Portal>
      <Dialog.Overlay class="dialog-overlay" />
      <div class="dialog-positioner">
        <Dialog.Content class="dialog-content max-w-xl">
          <div class="dialog-head">
            <Dialog.Title class="dialog-title">Supported devices</Dialog.Title>
            <Dialog.CloseButton class="icon-button ml-auto" aria-label="Close">
              <FiX class="size-4" />
            </Dialog.CloseButton>
          </div>
          <div class="grid gap-0.5 px-1 pb-2">
            <For each={supportedDevices}>
              {device => (
                <div class="flex items-center gap-3 rounded-control px-3 py-1.5">
                  <DeviceImage model={device.model} class="h-8 w-12" />
                  <span class="min-w-0 flex-1">
                    <span class="row-title block">{device.model}</span>
                    <span class="row-meta block tabular-nums">
                      {device.grid}
                      <Show when={device.extras}>{extras => `, ${extras()}`}</Show>
                    </span>
                  </span>
                  <span class="w-20 text-muted">{connectionLabels[device.connection]}</span>
                  <span class="w-24">
                    <Show
                      when={device.isImplemented}
                      fallback={(
                        <span class="inline-flex items-center gap-1.5 text-muted">
                          <span class="status-dot size-2 bg-slate-400 dark:bg-slate-600" aria-hidden="true" />
                          Planned
                        </span>
                      )}
                    >
                      <span class="status-label">
                        <span class="status-dot size-2 bg-emerald-500" aria-hidden="true" />
                        Supported
                      </span>
                    </Show>
                  </span>
                </div>
              )}
            </For>
          </div>
        </Dialog.Content>
      </div>
    </Dialog.Portal>
  </Dialog.Root>
);
