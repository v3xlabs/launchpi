import { Component } from "solid-js";

import { DeviceStatus } from "../api/inventory";

const statusClass: Record<DeviceStatus, string> = {
  connected: "bg-emerald-500",
  connecting: "bg-amber-500",
  unavailable: "bg-red-500",
  disabled: "bg-slate-400 dark:bg-slate-600",
};

export const statusWord: Record<DeviceStatus, string> = {
  connected: "Connected",
  connecting: "Connecting",
  unavailable: "Unavailable",
  disabled: "Disabled",
};

export const StatusDot: Component<{ status: DeviceStatus; label?: string; }> = properties => (
  <span
    class="status-dot size-2"
    classList={{ [statusClass[properties.status]]: true }}
    role="img"
    title={properties.label ?? statusWord[properties.status]}
    aria-label={properties.label ?? statusWord[properties.status]}
  />
);

/** A status shown as a dot next to its word, so colour is never the only signal. */
export const StatusLabel: Component<{ status: DeviceStatus; label?: string; }> = properties => (
  <span class="status-label">
    <span class="status-dot size-2" classList={{ [statusClass[properties.status]]: true }} aria-hidden="true" />
    {properties.label ?? statusWord[properties.status]}
  </span>
);
