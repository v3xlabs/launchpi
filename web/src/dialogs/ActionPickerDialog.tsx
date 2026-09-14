import * as Dialog from "@kobalte/core/dialog";
import { TbFillCircleX as TbX } from "solid-icons/tb";
import { Component, createMemo, createSignal, For, JSX, Show } from "solid-js";

import { Action } from "../api/inventory";
import { PluginCatalogue, PluginInstance } from "../api/plugins";
import { TextField } from "../components/fields";
import { useInventory } from "../context/InventoryContext";

type Offer = { key: string; name: string; meta: string; build: () => Action; };
type Group = { key: string; title: string; offers: Offer[]; };

/**
 * The daemon's own actions, written the way they read on a key rather than the way they are
 * tagged on the wire. Sleep and wake are one action apart, so they are offered as two entries: a
 * picker that made you choose the verb and then flip a boolean is the flow this replaces.
 */
const builtinGroups: Group[] = [
  {
    key: "display",
    title: "Display",
    offers: [
      {
        key: "sleep",
        name: "Sleep display",
        meta: "Backlight off until a key is pressed",
        build: () => ({
          type: "set_surface_display",
          surface_ids: [],
          include_triggering_surface: true,
          is_display_off: true,
        }),
      },
      {
        key: "wake",
        name: "Wake display",
        meta: "Back to its own brightness",
        build: () => ({
          type: "set_surface_display",
          surface_ids: [],
          include_triggering_surface: false,
          is_display_off: false,
        }),
      },
      {
        key: "brightness",
        name: "Set brightness",
        meta: "0 to 100 percent",
        build: () => ({
          type: "set_surface_brightness",
          surface_ids: [],
          include_triggering_surface: true,
          brightness: 100,
        }),
      },
    ],
  },
  {
    key: "panel",
    title: "Panel",
    offers: [
      {
        key: "change_panel",
        name: "Change panel",
        meta: "Replaces what this surface shows",
        build: () => ({ type: "change_panel", panel_id: "" }),
      },
      {
        key: "open_subpanel",
        name: "Open subpanel",
        meta: "Overlays a panel over part of the grid",
        build: () => ({
          type: "open_subpanel",
          panel_id: "",
          placement: "bottom_end",
          offset_columns: 0,
          offset_rows: 0,
        }),
      },
      {
        key: "close_subpanel",
        name: "Close subpanel",
        meta: "Drops the topmost overlay",
        build: () => ({ type: "close_subpanel" }),
      },
    ],
  },
  {
    key: "value",
    title: "Value",
    offers: [
      {
        key: "set_variable",
        name: "Set value",
        meta: "Writes a value other keys can read",
        build: () => ({ type: "set_variable", variable_name: "", value: "" }),
      },
      {
        key: "wait",
        name: "Wait",
        meta: "Pauses before the next action",
        build: () => ({ type: "wait", duration_ms: 200 }),
      },
    ],
  },
];

/**
 * An instance's own actions, flattened into the same list. Picking one here writes the instance
 * and the action name in a single step, instead of adding a blank integration call and then
 * choosing both from two selects that mean nothing until the first one is answered.
 */
const instanceGroup = (instance: PluginInstance, plugins: PluginCatalogue): Group | null => {
  const manifest = plugins.types.find(type => type.plugin_type === instance.plugin_type);

  if (manifest === undefined || manifest.actions.length === 0) return null;

  return {
    key: instance.integration_id,
    title: instance.display_name,
    offers: manifest.actions.map(definition => ({
      key: `${instance.integration_id}:${definition.name}`,
      name: definition.label,
      meta: definition.description ?? instance.integration_id,
      build: (): Action => ({
        type: "invoke_integration",
        integration_id: instance.integration_id,
        action_name: definition.name,
        parameters: {},
      }),
    })),
  };
};

export const ActionPickerDialog: Component<{
  trigger: JSX.Element;
  onChoose: (action: Action) => void;
}> = (properties) => {
  const store = useInventory();
  const [isOpen, setIsOpen] = createSignal(false);
  const [search, setSearch] = createSignal("");
  const [selectedGroupKey, setSelectedGroupKey] = createSignal<string | null>(null);

  const groups = createMemo<Group[]>(() => {
    const needle = search().trim()
      .toLowerCase();
    const matching = (offer: Offer) =>
      needle === "" || `${offer.name} ${offer.meta}`.toLowerCase().includes(needle);
    const running = store
      .plugins()
      .instances.filter(instance => instance.status.state === "running");
    const all = [
      ...builtinGroups,
      ...running
        .map(instance => instanceGroup(instance, store.plugins()))
        .filter((group): group is Group => group !== null),
    ];

    return all
      .map(group => ({ ...group, offers: group.offers.filter(matching) }))
      .filter(group => group.offers.length > 0);
  });
  const selectedGroup = createMemo(
    () => groups().find(group => group.key === selectedGroupKey()) ?? groups()[0],
  );

  const choose = (offer: Offer) => {
    properties.onChoose(offer.build());
    setIsOpen(false);
    setSearch("");
  };

  return (
    <Dialog.Root
      open={isOpen()}
      onOpenChange={(open) => {
        setIsOpen(open);
        setSearch("");

        if (open) setSelectedGroupKey(groups()[0]?.key ?? null);
      }}
    >
      <Dialog.Trigger as="div" class="contents">{properties.trigger}</Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay class="dialog-overlay" />
        <div class="dialog-positioner">
          <Dialog.Content class="dialog-content" data-size="wide">
            <div class="dialog-head">
              <Dialog.Title class="dialog-title">Action</Dialog.Title>
              <Dialog.CloseButton class="icon-button" aria-label="Close">
                <TbX class="h-3.5 w-3.5" />
              </Dialog.CloseButton>
            </div>
            <div class="dialog-body">
              <TextField
                label="Search"
                value={search()}
                placeholder="sleep, brightness, panel..."
                onChange={setSearch}
              />
              <div class="preset-picker">
                <nav class="preset-sidebar" aria-label="Action groups">
                  <For each={groups()}>
                    {group => (
                      <button
                        type="button"
                        class="preset-integration"
                        data-selected={group.key === selectedGroup()?.key}
                        onClick={() => setSelectedGroupKey(group.key)}
                      >
                        <span class="preset-integration-name">{group.title}</span>
                        <span class="preset-integration-count">{group.offers.length}</span>
                      </button>
                    )}
                  </For>
                </nav>
                <div class="action-list">
                  <Show
                    when={selectedGroup()}
                    fallback={<p class="empty">Nothing matches that.</p>}
                  >
                    {group => (
                      <div class="action-group">
                        <p class="preset-heading">{group().title}</p>
                        <For each={group().offers}>
                          {offer => (
                            <button
                              type="button"
                              class="action-option"
                              onClick={() => choose(offer)}
                            >
                              <span class="action-option-name">{offer.name}</span>
                              <span class="action-option-meta">{offer.meta}</span>
                            </button>
                          )}
                        </For>
                      </div>
                    )}
                  </Show>
                </div>
              </div>
            </div>
          </Dialog.Content>
        </div>
      </Dialog.Portal>
    </Dialog.Root>
  );
};
