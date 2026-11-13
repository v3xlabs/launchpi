import { Outlet } from "@tanstack/solid-router";
import { FiX } from "solid-icons/fi";
import { Component, createMemo, createSignal, onCleanup, Show } from "solid-js";

import { Sidebar } from "../components/Sidebar";
import {
  initialSidebarWidthPx,
  sidebarRailBelowPx,
  SidebarResizer,
  usableSidebarWidthPx,
} from "../components/SidebarResizer";
import { useInventory } from "../context/InventoryContext";

const sidebarId = "sidebar";

export const RootLayout: Component = () => {
  const store = useInventory();
  const [preferredWidthPx, setPreferredWidthPx] = createSignal(initialSidebarWidthPx());
  const [viewportPx, setViewportPx] = createSignal(document.documentElement.clientWidth);
  const sidebarWidthPx = createMemo(() => usableSidebarWidthPx(preferredWidthPx(), viewportPx()));
  const viewportObserver = new ResizeObserver(() => setViewportPx(document.documentElement.clientWidth));

  viewportObserver.observe(document.documentElement);
  onCleanup(() => viewportObserver.disconnect());

  return (
    <div
      class="shell"
      style={{ "--sidebar-w": `${sidebarWidthPx()}px` }}
      data-rail={sidebarWidthPx() < sidebarRailBelowPx ? "" : undefined}
    >
      <aside class="sidebar" aria-label="Sidebar">
        <Sidebar elementId={sidebarId} />
        <SidebarResizer sidebarId={sidebarId} widthPx={sidebarWidthPx()} onWidthChange={setPreferredWidthPx} />
      </aside>
      <main class="min-w-0">
        <Show when={store.error()}>
          {message => (
            <div class="page-inset pt-4">
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
          <p role="status" class="page-inset pt-8 text-muted">Loading...</p>
        </Show>
        <Outlet />
      </main>
    </div>
  );
};
