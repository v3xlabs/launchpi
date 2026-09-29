import { Link } from "@tanstack/solid-router";
import { FiMoon, FiSun } from "solid-icons/fi";
import { Component, createEffect, createSignal, For, Show } from "solid-js";

import { useInventory } from "../context/InventoryContext";
import { ChangesPopover } from "./ChangesPopover";

const sections = [
  { to: "/devices", label: "Devices" },
  { to: "/panels", label: "Panels" },
  { to: "/plugins", label: "Plugins" },
  { to: "/values", label: "Values" },
] as const;

const themeKey = "launchpi-theme";

const ThemeToggle: Component = () => {
  const [isDark, setIsDark] = createSignal(document.documentElement.classList.contains("dark"));
  const toggle = () => {
    const isNextDark = !isDark();

    document.documentElement.classList.toggle("dark", isNextDark);
    localStorage.setItem(themeKey, isNextDark ? "dark" : "light");
    setIsDark(isNextDark);
  };
  const label = () => (isDark() ? "Switch to light mode" : "Switch to dark mode");

  return (
    <button
      type="button"
      class="icon-button"
      onClick={toggle}
      aria-label={label()}
      title={label()}
    >
      <Show when={isDark()} fallback={<FiMoon class="size-4" />}>
        <FiSun class="size-4" />
      </Show>
    </button>
  );
};

export const AppHeader: Component = () => {
  const store = useInventory();
  const [hasConnected, setHasConnected] = createSignal(false);

  // The socket opens a moment after the first fetch; only a dropped connection is news.
  createEffect(() => {
    if (store.isConnected()) setHasConnected(true);
  });

  return (
    <header class="mx-auto flex max-w-5xl items-center gap-3 px-6 py-2">
      <Link to="/devices" class="flex items-center gap-2">
        <img src="/icon.svg" class="size-6" alt="" />
        <span class="text-sm font-semibold tracking-tight">Launchpi</span>
      </Link>
      <span class="text-slate-300 dark:text-slate-600" aria-hidden="true">/</span>
      <nav class="-my-2 flex items-center gap-3" aria-label="Sections">
        <For each={sections}>
          {section => (
            <Link to={section.to} class="tab py-4">
              {section.label}
            </Link>
          )}
        </For>
      </nav>
      <div class="ml-auto flex items-center gap-2">
        <Show when={hasConnected() && !store.isConnected()}>
          <span role="status" class="inline-flex items-center gap-1.5 text-amber-700 dark:text-amber-400">
            <span class="status-dot size-2 bg-amber-500" aria-hidden="true" />
            Reconnecting...
          </span>
        </Show>
        <Show when={store.inventory().config.mode === "declarative" && store.inventory().config.changes > 0}>
          <ChangesPopover />
        </Show>
        <ThemeToggle />
      </div>
    </header>
  );
};
