import { Link } from "@tanstack/solid-router";
import {
  TbFillDeviceRemote as TbDeviceRemote,
  TbFillLayoutGrid as TbLayoutGrid,
  TbFillPuzzle as TbPuzzle,
  TbFillTag as TbTag,
} from "solid-icons/tb";
import { Component } from "solid-js";

import { useInventory } from "../context/InventoryContext";

const DaemonStatus: Component = () => {
  const store = useInventory();
  const label = () => (store.isConnected() ? "Daemon online" : "Reconnecting to daemon");

  return (
    <span
      class="rail-status"
      role="status"
      aria-label={label()}
      title={label()}
    >
      <span
        classList={{
          "status-dot": true,
          "h-2.5 w-2.5": true,
          "bg-emerald-500": store.isConnected(),
          "bg-amber-500": !store.isConnected(),
        }}
      />
    </span>
  );
};

export const IconRail: Component = () => (
  <nav class="rail" aria-label="Sections">
    <Link
      to="/"
      // class="rail-item"
      aria-label="Launchpi"
      title="Launchpi"
    >
      <img src="icon.svg" class="size-8" />
    </Link>
    <Link
      to="/devices"
      class="rail-item"
      aria-label="Devices"
      title="Devices"
    >
      <TbDeviceRemote class="size-6" />
    </Link>
    <Link
      to="/panels"
      class="rail-item"
      aria-label="Panels"
      title="Panels"
    >
      <TbLayoutGrid class="size-5" />
    </Link>
    <Link
      to="/plugins"
      class="rail-item"
      aria-label="Plugins"
      title="Plugins"
    >
      <TbPuzzle class="size-5" />
    </Link>
    <Link
      to="/values"
      class="rail-item"
      aria-label="Values"
      title="Values"
    >
      <TbTag class="size-5" />
    </Link>
    <DaemonStatus />
  </nav>
);
