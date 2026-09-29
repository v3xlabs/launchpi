import { Outlet } from "@tanstack/solid-router";
import { FiX } from "solid-icons/fi";
import { Component, Show } from "solid-js";

import { AppHeader } from "../components/AppHeader";
import { useInventory } from "../context/InventoryContext";

export const RootLayout: Component = () => {
  const store = useInventory();

  return (
    <div class="min-h-screen">
      <AppHeader />
      <Show when={store.error()}>
        {message => (
          <div class="mx-auto max-w-5xl px-6 pt-4">
            <div role="alert" class="alert">
              <span class="flex-1">{message()}</span>
              <button
                type="button"
                class="icon-button text-red-700 hover:bg-red-100 dark:text-red-400 dark:hover:bg-red-950/60"
                onClick={() => store.setError(null)}
                aria-label="Dismiss error"
              >
                <FiX class="size-4" />
              </button>
            </div>
          </div>
        )}
      </Show>
      <Show when={store.isLoading()}>
        <p role="status" class="mx-auto max-w-5xl px-6 pt-8 text-muted">Loading...</p>
      </Show>
      <main>
        <Outlet />
      </main>
    </div>
  );
};
