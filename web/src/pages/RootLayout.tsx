import { Outlet } from "@tanstack/solid-router";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, Show } from "solid-js";

import { ContextSidebar } from "../components/ContextSidebar";
import { IconRail } from "../components/IconRail";
import { useInventory } from "../context/InventoryContext";

export const RootLayout: Component = () => {
  const store = useInventory();

  return (
    <div class="app-shell">
      <IconRail />
      <ContextSidebar />
      <main class="min-w-0">
        <Show when={store.error()}>
          {message => (
            <div class="px-4 pt-4 sm:px-6">
              <div role="alert" class="alert">
                <span>{message()}</span>
                <button
                  type="button"
                  onClick={() => store.setError(null)}
                  aria-label="Dismiss error"
                >
                  <TbX class="h-4 w-4" />
                </button>
              </div>
            </div>
          )}
        </Show>
        <Show when={store.isLoading()}>
          <p class="px-4 pt-4 text-xs text-neutral-500 sm:px-6">Loading workspace...</p>
        </Show>
        <Outlet />
      </main>
    </div>
  );
};
