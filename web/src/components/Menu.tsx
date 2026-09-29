import * as DropdownMenu from "@kobalte/core/dropdown-menu";
import { FiMoreHorizontal } from "solid-icons/fi";
import { Component, JSX, ParentComponent } from "solid-js";

import { fetchText } from "../api/guards";
import { useInventory } from "../context/InventoryContext";

/** The `...` glyph that holds an item's secondary and destructive actions. */
export const OverflowMenu: ParentComponent<{ label: string; }> = properties => (
  <DropdownMenu.Root>
    <DropdownMenu.Trigger class="icon-button" aria-label={properties.label}>
      <FiMoreHorizontal class="size-4" />
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content class="popover min-w-44">{properties.children}</DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
);

export const MenuItem: Component<{
  children: JSX.Element;
  onSelect: () => void;
  isDanger?: boolean;
  isDisabled?: boolean;
}> = properties => (
  <DropdownMenu.Item
    class="menu-item"
    data-danger={properties.isDanger === true ? "" : undefined}
    disabled={properties.isDisabled}
    onSelect={properties.onSelect}
  >
    {properties.children}
  </DropdownMenu.Item>
);

export const MenuSeparator: Component = () => <DropdownMenu.Separator class="menu-separator" />;

/**
 * Both copy formats for one exported entry. `path` is the entry's `/config` route, which renders
 * TOML by default and a `services.launchpi.settings` attribute set with `?format=nix`.
 */
export const CopyConfigItems: Component<{ path: string; }> = (properties) => {
  const store = useInventory();

  return (
    <>
      <MenuItem onSelect={() => void store.copyToClipboard(() => fetchText(properties.path))}>
        Copy as TOML
      </MenuItem>
      <MenuItem onSelect={() => void store.copyToClipboard(() => fetchText(`${properties.path}?format=nix`))}>
        Copy as Nix
      </MenuItem>
    </>
  );
};
