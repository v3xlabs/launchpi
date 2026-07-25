import { Link } from "@tanstack/solid-router";
import {
  TbFillCirclePlus as TbPlus,
  TbFillClipboard as TbCopy,
  TbFillTrash as TbTrash,
} from "solid-icons/tb";
import { Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";

import { fetchFullConfig as fetchFullConfig } from "../api/inventory";
import {
  coerceConfigValue,
  ConfigField,
  fetchInstanceConfig,
  PluginInstance,
  statusLabel,
  statusReason,
  statusTone,
  variableReference,
  withoutUntouchedSecrets,
} from "../api/plugins";
import { ConfigFieldInput } from "../components/fields";
import { StatusDot } from "../components/StatusDot";
import { useInventory } from "../context/InventoryContext";
import { AddPluginDialog } from "../dialogs/AddPluginDialog";

const InstanceRow: Component<{ instance: PluginInstance; }> = (properties) => {
  const store = useInventory();

  return (
    <div class="row">
      <StatusDot status={statusTone(properties.instance.status)} />
      <div class="row-main">
        <div class="min-w-0 flex-1">
          <Link to="/plugins/$integrationId" params={{ integrationId: properties.instance.integration_id }}>
            <p class="row-title">{properties.instance.display_name}</p>
          </Link>
          <p class="row-meta">
            <span class="mono">{properties.instance.integration_id}</span>
            <span class="meta-sep">-</span>
            {statusLabel(properties.instance.status)}
            <Show when={statusReason(properties.instance.status)}>
              {reason => (
                <>
                  <span class="meta-sep">-</span>
                  {reason()}
                </>
              )}
            </Show>
          </p>
        </div>
      </div>
      <button
        type="button"
        class="secondary-button"
        disabled={store.isSaving()}
        onClick={() =>
          void store.updatePluginInstance(properties.instance.integration_id, {
            is_enabled: !properties.instance.is_enabled,
          })}
      >
        {properties.instance.is_enabled ? "Disable" : "Enable"}
      </button>
    </div>
  );
};

const PluginsOverview: Component = () => {
  const store = useInventory();

  return (
    <div class="page">
      <div class="page-head">
        <div>
          <h1 class="page-title">Plugins</h1>
          <p class="page-subtitle">
            Each plugin can be configured more than once. An instance is one file under
            {" "}
            <span class="mono">plugins/</span>
            .
          </p>
        </div>
        <div class="flex gap-2">
          <AddPluginDialog
            trigger={(
              <button type="button" class="primary-button">
                <TbPlus class="h-3.5 w-3.5" />
                Add plugin
              </button>
            )}
          />
          <CopyConfigButton />
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <p class="card-title">Configured</p>
          <span class="chip chip-muted">{store.plugins().instances.length}</span>
        </div>
        <div class="card-body">
          <Show
            when={store.plugins().instances.length > 0}
            fallback={<p class="empty">No plugin instances yet.</p>}
          >
            <div class="rows">
              <For each={store.plugins().instances}>
                {instance => <InstanceRow instance={instance} />}
              </For>
            </div>
          </Show>
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <p class="card-title">Available</p>
        </div>
        <div class="card-body">
          <div class="rows">
            <For each={store.plugins().types}>
              {manifest => (
                <div class="row">
                  <div class="row-main">
                    <div class="min-w-0 flex-1">
                      <p class="row-title">{manifest.display_name}</p>
                      <p class="row-meta">{manifest.description}</p>
                      <p class="row-meta">
                        <span class="chip chip-muted">
                          {manifest.actions.length}
                          {" actions"}
                        </span>
                        <span class="chip chip-muted">
                          {manifest.feedbacks.length}
                          {" feedbacks"}
                        </span>
                      </p>
                    </div>
                  </div>
                  <AddPluginDialog
                    trigger={(
                      <button type="button" class="secondary-button">
                        <TbPlus class="h-3.5 w-3.5" />
                        Add
                      </button>
                    )}
                  />
                </div>
              )}
            </For>
          </div>
        </div>
      </div>
    </div>
  );
};

const CopyConfigButton: Component = () => {
  const store = useInventory();
  const [copied, setCopied] = createSignal(false);

  return (
    <button
      type="button"
      class="secondary-button"
      onClick={() =>
        void store.copyToClipboard(async () => {
          const text = await fetchFullConfig();

          setCopied(true);
          setTimeout(() => setCopied(false), 1500);

          return text;
        })}
    >
      <TbCopy class="h-3.5 w-3.5" />
      {copied() ? "Copied" : "Copy all TOML"}
    </button>
  );
};

const InstanceDetail: Component<{ integrationId: string; }> = (properties) => {
  const store = useInventory();
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
  const [copied, setCopied] = createSignal(false);

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

  const liveVariables = createMemo(() =>
    Object.entries(store.variables)
      .filter(([key]) => key.startsWith(`${properties.integrationId}:`))
      .map(([key, value]) => ({ name: key.slice(properties.integrationId.length + 1), value })),
  );

  return (
    <Show when={instance()} fallback={<div class="page"><p class="empty">This plugin instance was not found.</p></div>}>
      {found => (
        <div class="page">
          <div class="page-head">
            <div>
              <p class="breadcrumb">
                <Link to="/plugins">Plugins</Link>
              </p>
              <h1 class="page-title">{found().display_name}</h1>
              <p class="meta-line">
                <span class="mono">{found().integration_id}</span>
                <span class="meta-sep">-</span>
                <StatusDot status={statusTone(found().status)} />
                {statusLabel(found().status)}
              </p>
            </div>
            <div class="flex gap-2">
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
              <button
                type="button"
                class="secondary-button"
                onClick={() =>
                  void store.copyToClipboard(async () => {
                    const text = await fetchInstanceConfig(found().integration_id);

                    setCopied(true);
                    setTimeout(() => setCopied(false), 1500);

                    return text;
                  })}
              >
                <TbCopy class="h-3.5 w-3.5" />
                {copied() ? "Copied" : "Copy TOML"}
              </button>
              <button
                type="button"
                class="danger-button"
                aria-label="Delete instance"
                onClick={() => void store.deletePluginInstance(found().integration_id)}
              >
                <TbTrash class="h-3.5 w-3.5" />
              </button>
            </div>
          </div>

          <Show when={statusReason(found().status)}>
            {reason => <p class="alert">{reason()}</p>}
          </Show>

          <div class="editor">
            <div class="grid gap-4">
              <div class="card">
                <div class="card-head">
                  <p class="card-title">Configuration</p>
                  <button
                    type="button"
                    class="primary-button"
                    disabled={!draft.dirty || store.isSaving()}
                    onClick={() => void save()}
                  >
                    Save
                  </button>
                </div>
                <div class="card-body">
                  <For each={manifest()?.config_schema ?? []}>
                    {field => (
                      <ConfigFieldInput
                        field={field}
                        value={draft.values[field.key]}
                        onChange={raw => setField(field, raw)}
                      />
                    )}
                  </For>
                  <p class="hint">
                    Secrets are never sent back to the browser. Leave one blank to keep what is
                    already configured.
                  </p>
                </div>
              </div>

              <div class="card">
                <div class="card-head">
                  <p class="card-title">Actions</p>
                </div>
                <div class="card-body">
                  <div class="rows">
                    <For each={manifest()?.actions ?? []}>
                      {action => (
                        <div class="row">
                          <div class="row-main">
                            <p class="row-title">{action.label}</p>
                            <p class="row-meta">
                              <span class="mono">{action.name}</span>
                              <Show when={action.description}>
                                {description => (
                                  <>
                                    <span class="meta-sep">-</span>
                                    {description()}
                                  </>
                                )}
                              </Show>
                            </p>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </div>
            </div>

            <div class="grid gap-4">
              <div class="card">
                <div class="card-head">
                  <p class="card-title">Variables</p>
                  <span class="chip chip-muted">{liveVariables().length}</span>
                </div>
                <div class="card-body">
                  <Show
                    when={liveVariables().length > 0}
                    fallback={<p class="empty">Nothing published yet.</p>}
                  >
                    <div class="rows">
                      <For each={liveVariables()}>
                        {variable => (
                          <div class="row">
                            <div class="row-main">
                              <p class="row-title mono">
                                {variableReference(found().integration_id, variable.name)}
                              </p>
                              <p class="row-meta">{variable.value}</p>
                            </div>
                          </div>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              </div>

              <div class="card">
                <div class="card-head">
                  <p class="card-title">Feedbacks</p>
                </div>
                <div class="card-body">
                  <div class="rows">
                    <For each={manifest()?.feedbacks ?? []}>
                      {feedback => (
                        <div class="row">
                          <div class="row-main">
                            <p class="row-title">{feedback.label}</p>
                            <p class="row-meta mono">{feedback.name}</p>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              </div>
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
