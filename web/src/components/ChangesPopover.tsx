import * as Popover from "@kobalte/core/popover";
import { FiChevronDown, FiCopy } from "solid-icons/fi";
import { Component, createResource, createSignal, For, Show } from "solid-js";

import { ConfigChange, fetchConfigChanges } from "../api/config";
import { useInventory } from "../context/InventoryContext";
import { InfoTip } from "./InfoTip";

const kindWord: Record<ConfigChange["kind"], string> = {
  device: "Device",
  panel: "Panel",
  plugin: "Plugin",
  value: "Value",
};

const changeTone: Record<ConfigChange["change"], { word: string; text: string; dot: string; }> = {
  added: { word: "Added", text: "text-emerald-700 dark:text-emerald-400", dot: "bg-emerald-500" },
  changed: { word: "Changed", text: "text-amber-700 dark:text-amber-400", dot: "bg-amber-500" },
  removed: { word: "Removed", text: "text-red-700 dark:text-red-400", dot: "bg-red-500" },
};

/** Only shown when the configuration is read-only, where every edit is lost on restart. */
export const ChangesPopover: Component = () => {
  const store = useInventory();
  const [isOpen, setIsOpen] = createSignal(false);
  const count = () => store.inventory().config.changes;
  const [changes] = createResource(() => (isOpen() ? count() : null), fetchConfigChanges);

  return (
    <Popover.Root
      open={isOpen()}
      onOpenChange={setIsOpen}
      placement="bottom-end"
      gutter={6}
    >
      <Popover.Trigger class="inline-flex items-center gap-1.5 rounded-control bg-amber-50 px-2.5 py-1 text-sm font-medium text-amber-700 transition-colors hover:bg-amber-100 dark:bg-amber-950/40 dark:text-amber-400 dark:hover:bg-amber-950/70">
        <span class="status-dot size-2 bg-amber-500" aria-hidden="true" />
        <span class="tabular-nums">{count()}</span>
        {count() === 1 ? "temporary change" : "temporary changes"}
        <FiChevronDown class="size-3.5" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content class="popover w-96">
          <div class="flex items-center gap-1.5 px-3 pt-2 pb-1">
            <Popover.Title class="font-semibold">Temporary changes</Popover.Title>
            <InfoTip>
              Launchpi runs from a read-only configuration. These changes stay active until it
              restarts. Copy them into services.launchpi.settings to keep them.
            </InfoTip>
            <span class="ml-auto text-muted">Reset on restart</span>
          </div>
          <Show when={changes.error}>
            <p role="alert" class="px-3 py-2 error-text">Unable to load the changes.</p>
          </Show>
          <Show when={changes.latest} fallback={<p role="status" class="px-3 py-2 text-muted">Loading...</p>}>
            {current => (
              <>
                <ul class="py-1">
                  <For each={current().changes}>
                    {change => (
                      <li class="flex items-center gap-3 rounded-control px-3 py-2">
                        <span class="w-14 text-muted">{kindWord[change.kind]}</span>
                        <span class="min-w-0 flex-1 truncate font-medium">{change.name}</span>
                        <span class="inline-flex items-center gap-1.5" classList={{ [changeTone[change.change].text]: true }}>
                          <span class="status-dot size-2" classList={{ [changeTone[change.change].dot]: true }} aria-hidden="true" />
                          {changeTone[change.change].word}
                        </span>
                      </li>
                    )}
                  </For>
                </ul>
                <div class="flex items-center gap-2 border-t border-hairline px-3 pt-3 pb-2">
                  <button
                    type="button"
                    class="primary-button"
                    onClick={() => void store.copyToClipboard(async () => current().nix)}
                  >
                    <FiCopy class="size-3.5" />
                    Copy as Nix
                  </button>
                  <button
                    type="button"
                    class="secondary-button"
                    onClick={() => void store.copyToClipboard(async () => current().toml)}
                  >
                    Copy TOML
                  </button>
                </div>
              </>
            )}
          </Show>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
};
