import { Component } from "solid-js";

import { DeviceStatus } from "../api/inventory";

const statusClass: Record<DeviceStatus, string> = {
  connected: "bg-emerald-500",
  connecting: "bg-amber-500",
  unavailable: "bg-rose-500",
  disabled: "bg-neutral-600",
};

export const StatusDot: Component<{ status: DeviceStatus; class?: string; }> = properties => (
  <span
    classList={{
      "status-dot": true,
      "h-2 w-2": properties.class === undefined,
      [statusClass[properties.status]]: true,
      [properties.class ?? ""]: true,
    }}
    aria-hidden="true"
  />
);

export const StatusLabel: Component<{ status: DeviceStatus; }> = properties => (
  <span class="status-label">
    <StatusDot status={properties.status} />
    {properties.status}
  </span>
);
