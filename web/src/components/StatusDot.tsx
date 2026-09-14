import { Component } from "solid-js";

import { DeviceStatus } from "../api/inventory";

const statusClass: Record<DeviceStatus, string> = {
  connected: "bg-emerald-500",
  connecting: "bg-amber-500",
  unavailable: "bg-rose-500",
  disabled: "bg-neutral-600",
};

/**
 * The dot carries its own label, so a status shown as a colour is still readable by hovering it or
 * by a screen reader, and the row beside it does not have to spell the word out a second time.
 */
export const StatusDot: Component<{ status: DeviceStatus; label?: string; class?: string; }> = properties => (
  <span
    classList={{
      "status-dot": true,
      "h-2 w-2": properties.class === undefined,
      [statusClass[properties.status]]: true,
      [properties.class ?? ""]: true,
    }}
    role="img"
    title={properties.label ?? properties.status}
    aria-label={properties.label ?? properties.status}
  />
);

export const StatusLabel: Component<{ status: DeviceStatus; }> = properties => (
  <span class="status-label">
    <StatusDot status={properties.status} />
    {properties.status}
  </span>
);
