import * as Dialog from "@kobalte/core/dialog";
import { Link, useNavigate } from "@tanstack/solid-router";
import { FiPackage, FiPlay } from "solid-icons/fi";
import { Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";

import {
  ActionDefinition,
  coerceConfigValue,
  ConfigField,
  PluginInstance,
  PluginManifest,
  statusLabel,
  statusReason,
  statusTone,
  variableReference,
  withoutUntouchedSecrets,
} from "../api/plugins";
import { ConfigFieldInput } from "../components/fields";
import { CopyConfigItems, MenuItem, MenuSeparator, OverflowMenu } from "../components/Menu";
import { MetaSeparator, PageHeader } from "../components/PageHeader";
import { StatusLabel } from "../components/StatusDot";
import { useInventory } from "../context/InventoryContext";
import { AddPluginDialog } from "../dialogs/AddPluginDialog";

/** An instance left at its default name reads as its id, so the plugin's own name says more. */
export const instanceTitle = (instance: PluginInstance, types: PluginManifest[]): string =>
  (instance.display_name === instance.integration_id
    ? types.find(type => type.plugin_type === instance.plugin_type)?.display_name ?? instance.display_name
    : instance.display_name);

const InstanceRow: Component<{ instance: PluginInstance; }> = (properties) => {
  const store = useInventory();
  const title = () => instanceTitle(properties.instance, store.plugins().types);

  return (
    <div class="row transition-colors hover:bg-raised/60">
      <Link
        to="/plugins/$integrationId"
        params={{ integrationId: properties.instance.integration_id }}
        class="flex min-w-0 flex-1 items-center gap-4"
      >
        <span class="min-w-0 flex-1">
          <span class="flex items-baseline gap-2">
            <span class="row-title">{title()}</span>
            <code class="mono">{properties.instance.integration_id}</code>
          </span>
          <Show when={statusReason(properties.instance.status)}>
            {reason => <span class="error-text mt-0.5 block truncate" title={reason()}>{reason()}</span>}
          </Show>
        </span>
        <span class="w-24 shrink-0">
          <Show
            when={properties.instance.status.state === "error"}
            fallback={(
              <StatusLabel
                status={statusTone(properties.instance.status)}
                label={statusLabel(properties.instance.status)}
              />
            )}
          >
            <span class="error-text inline-flex items-center gap-1.5">
              <span class="status-dot size-2 bg-red-500" aria-hidden="true" />
              Error
            </span>
          </Show>
        </span>
      </Link>
      <OverflowMenu label={`More actions for ${title()}`}>
        <MenuItem
          isDisabled={store.isSaving()}
          onSelect={() =>
            void store.updatePluginInstance(properties.instance.integration_id, {
              is_enabled: !properties.instance.is_enabled,
            })}
        >
          {properties.instance.is_enabled ? "Disable" : "Enable"}
        </MenuItem>
        <CopyConfigItems path={`/api/plugins/${encodeURIComponent(properties.instance.integration_id)}/config`} />
        <MenuSeparator />
        <MenuItem isDanger onSelect={() => void store.deletePluginInstance(properties.instance.integration_id)}>
          Remove
        </MenuItem>
      </OverflowMenu>
    </div>
  );
};

const PluginsOverview: Component = () => {
  const store = useInventory();

  return (
    <div class="page">
      <PageHeader
        mark={<FiPackage class="size-5" />}
        title="Plugins"
        meta={(
          <span>
            <span class="tabular-nums">{store.plugins().instances.length}</span>
            {" "}
            added
          </span>
        )}
        actions={<AddPluginDialog />}
      />

      <Show
        when={store.plugins().instances.length > 0}
        fallback={<p class="surface empty">No plugins yet.</p>}
      >
        <div class="surface rows overflow-hidden">
          <For each={store.plugins().instances}>
            {instance => <InstanceRow instance={instance} />}
          </For>
        </div>
      </Show>
    </div>
  );
};

const RunActionButton: Component<{ integrationId: string; action: ActionDefinition; }> = (properties) => {
  const store = useInventory();
  const [isOpen, setIsOpen] = createSignal(false);
  const [parameters, setParameters] = createStore<Record<string, unknown>>({});

  const run = async (): Promise<void> => {
    const succeeded = await store.runPluginAction(properties.integrationId, properties.action.name, {
      ...parameters,
    });

    if (succeeded) setIsOpen(false);
  };

  return (
    <Show
      when={properties.action.parameters.length > 0}
      fallback={(
        <button
          type="button"
          class="secondary-button"
          disabled={store.isSaving()}
          onClick={() => void store.runPluginAction(properties.integrationId, properties.action.name, {})}
        >
          <FiPlay class="size-4" />
          Run
        </button>
      )}
    >
      <Dialog.Root open={isOpen()} onOpenChange={setIsOpen}>
        <Dialog.Trigger class="secondary-button">
          <FiPlay class="size-4" />
          Run
        </Dialog.Trigger>
        <Dialog.Portal>
          <Dialog.Overlay class="dialog-overlay" />
          <div class="dialog-positioner">
            <Dialog.Content class="dialog-content">
              <div class="dialog-head">
                <Dialog.Title class="dialog-title">{properties.action.label}</Dialog.Title>
              </div>
              <div class="dialog-body">
                <For each={properties.action.parameters}>
                  {field => (
                    <ConfigFieldInput
                      field={field}
                      supportsReferences
                      integrationId={properties.integrationId}
                      value={parameters[field.key]}
                      onChange={raw => setParameters(field.key, coerceConfigValue(field, raw))}
                    />
                  )}
                </For>
              </div>
              <div class="dialog-actions">
                <Dialog.CloseButton class="secondary-button">Cancel</Dialog.CloseButton>
                <button
                  type="button"
                  class="primary-button"
                  disabled={store.isSaving()}
                  onClick={() => void run()}
                >
                  {store.isSaving() ? "Running..." : "Run"}
                </button>
              </div>
            </Dialog.Content>
          </div>
        </Dialog.Portal>
      </Dialog.Root>
    </Show>
  );
};

const InstanceDetail: Component<{ integrationId: string; }> = (properties) => {
  const store = useInventory();
  const navigate = useNavigate();
  const instance = createMemo(() =>
    store.plugins().instances.find(entry => entry.integration_id === properties.integrationId) ?? null,
  );
  const manifest = createMemo(() => {
    const found = instance();

    return found === null
      ? null
      : store.plugins().types.find(type => type.plugin_type === found.plugin_type) ?? null;
  });
  const [draft, setDraft] = createStore<{ values: Record<string, unknown>; dirty: boolean; }>({
    values: {},
    dirty: false,
  });

  // Reseed whenever the instance is replaced, which is every save, so the form follows what the
  // daemon actually stored rather than what was typed.
  createEffect(() => {
    const found = instance();

    if (found !== null) setDraft({ values: { ...found.config }, dirty: false });
  });

  const setField = (field: ConfigField, raw: string | boolean): void =>
    setDraft(
      produce((state) => {
        state.values[field.key] = coerceConfigValue(field, raw);
        state.dirty = true;
      }),
    );

  const save = async (): Promise<void> => {
    const schema = manifest()?.config_schema ?? [];
    const saved = await store.updatePluginInstance(properties.integrationId, {
      config: withoutUntouchedSecrets(schema, draft.values),
    });

    if (saved) setDraft("dirty", false);
  };

  const remove = async (): Promise<void> => {
    if (await store.deletePluginInstance(properties.integrationId)) void navigate({ to: "/plugins" });
  };

  const liveVariables = createMemo(() =>
    Object.entries(store.variables)
      .filter(([key]) => key.startsWith(`${properties.integrationId}:`))
      .map(([key, value]) => ({ name: key.slice(properties.integrationId.length + 1), value })),
  );

  return (
    <Show when={instance()} fallback={<div class="page"><p class="surface empty">This plugin was not found.</p></div>}>
      {found => (
        <div class="page">
          <PageHeader
            mark={<FiPackage class="size-5" />}
            title={<span class="truncate">{instanceTitle(found(), store.plugins().types)}</span>}
            meta={(
              <>
                <StatusLabel status={statusTone(found().status)} label={statusLabel(found().status)} />
                <MetaSeparator />
                <code class="mono">{found().integration_id}</code>
              </>
            )}
            actions={(
              <>
                <button
                  type="button"
                  class="secondary-button"
                  disabled={store.isSaving()}
                  onClick={() =>
                    void store.updatePluginInstance(found().integration_id, {
                      is_enabled: !found().is_enabled,
                    })}
                >
                  {found().is_enabled ? "Disable" : "Enable"}
                </button>
                <OverflowMenu label={`More actions for ${instanceTitle(found(), store.plugins().types)}`}>
                  <CopyConfigItems path={`/api/plugins/${encodeURIComponent(found().integration_id)}/config`} />
                  <MenuSeparator />
                  <MenuItem isDanger onSelect={() => void remove()}>Remove</MenuItem>
                </OverflowMenu>
              </>
            )}
          />

          <Show when={statusReason(found().status)}>
            {reason => <p class="alert" role="alert">{reason()}</p>}
          </Show>

          <div class="grid grid-cols-[minmax(0,1fr)_20rem] items-start gap-4">
            <section>
              <div class="section-head">
                <h2 class="label-sm">Configuration</h2>
              </div>
              <div class="surface">
                <div class="grid gap-4 p-4">
                  <For
                    each={manifest()?.config_schema ?? []}
                    fallback={<p class="text-muted">Nothing to configure.</p>}
                  >
                    {field => (
                      <ConfigFieldInput
                        field={field}
                        integrationId={found().integration_id}
                        value={draft.values[field.key]}
                        onChange={raw => setField(field, raw)}
                      />
                    )}
                  </For>
                </div>
                <div class="flex items-center justify-end gap-2 border-t border-hairline px-4 py-3">
                  <Show when={draft.dirty} fallback={<span class="mr-auto text-muted">No unsaved changes</span>}>
                    <span class="unsaved mr-auto">Unsaved changes</span>
                  </Show>
                  <button
                    type="button"
                    class="primary-button"
                    disabled={!draft.dirty || store.isSaving()}
                    onClick={() => void save()}
                  >
                    {store.isSaving() ? "Saving..." : "Save"}
                  </button>
                </div>
              </div>
            </section>

            <div class="grid gap-8">
              <Show when={(manifest()?.actions ?? []).length > 0}>
                <section>
                  <div class="section-head">
                    <h2 class="label-sm">Actions</h2>
                  </div>
                  <div class="surface rows">
                    <For each={manifest()?.actions ?? []}>
                      {action => (
                        <div class="row gap-3">
                          <span class="min-w-0 flex-1" title={action.description ?? undefined}>
                            <span class="row-title block">{action.label}</span>
                            <code class="mono">{action.name}</code>
                          </span>
                          <RunActionButton integrationId={found().integration_id} action={action} />
                        </div>
                      )}
                    </For>
                  </div>
                </section>
              </Show>

              <section>
                <div class="section-head">
                  <h2 class="label-sm">Values</h2>
                </div>
                <Show
                  when={liveVariables().length > 0}
                  fallback={<p class="surface empty">None yet.</p>}
                >
                  <div class="surface rows">
                    <For each={liveVariables()}>
                      {variable => (
                        <div class="row py-2">
                          <code class="mono min-w-0 flex-1 truncate">
                            {variableReference(found().integration_id, variable.name)}
                          </code>
                          <span class="max-w-[50%] truncate text-right tabular-nums">{variable.value}</span>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </section>
            </div>
          </div>
        </div>
      )}
    </Show>
  );
};

export const PluginsPage: Component<{ integrationId?: string; }> = properties => (
  <Show when={properties.integrationId} fallback={<PluginsOverview />}>
    {integrationId => <InstanceDetail integrationId={integrationId()} />}
  </Show>
);
