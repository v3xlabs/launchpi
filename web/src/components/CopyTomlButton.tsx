import { TbFillClipboard as TbCopy } from "solid-icons/tb";
import { Component, createSignal } from "solid-js";

import { useInventory } from "../context/InventoryContext";

/**
 * Copies a configuration document to the clipboard. Every export endpoint emits the same schema the
 * daemon loads, so what lands on the clipboard can be pasted into a file or a Nix generator as-is.
 */
export const CopyTomlButton: Component<{ label?: string; load: () => Promise<string>; }> = (
  properties,
) => {
  const store = useInventory();
  const [copied, setCopied] = createSignal(false);

  return (
    <button
      type="button"
      class="secondary-button"
      onClick={() =>
        void store.copyToClipboard(async () => {
          const text = await properties.load();

          setCopied(true);
          setTimeout(() => setCopied(false), 1500);

          return text;
        })}
    >
      <TbCopy class="h-3.5 w-3.5" />
      {copied() ? "Copied" : properties.label ?? "Copy TOML"}
    </button>
  );
};
