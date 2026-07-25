import { Component, createResource, createUniqueId, For, Match, Show, Switch } from "solid-js";

import { ColorBinding } from "../api/inventory";
import { ConfigField, fetchLookup, LookupOption } from "../api/plugins";
import { fromHex, isReference, toHex } from "../utils/rendered";

export const TextField: Component<{
  label: string;
  value: string;
  placeholder?: string;
  onChange: (value: string) => void;
}> = properties => (
  <label class="field-label">
    {properties.label}
    <input
      class="field-input"
      value={properties.value}
      placeholder={properties.placeholder}
      onInput={event => properties.onChange(event.currentTarget.value)}
    />
  </label>
);

/**
 * A colour is either picked or bound to a value. The link toggle is what makes
 * "make this key the colour of that light" reachable without hand-editing TOML.
 */
export const ColorField: Component<{
  label: string;
  value: ColorBinding | null;
  fallback: string;
  /** Dials take a fixed colour only; binding them is a later phase. */
  bindable?: boolean;
  onChange: (value: ColorBinding) => void;
}> = (properties) => {
  const bound = () => isReference(properties.value);

  return (
    <div class="grid gap-1">
      <div class="flex items-center justify-between gap-2">
        <span class="field-label">{properties.label}</span>
        <Show when={properties.bindable !== false}>
          <button
            type="button"
            class="link-button"
            title={bound() ? "Use a fixed colour" : "Bind to a value"}
            onClick={() => properties.onChange(bound() ? fromHex(properties.fallback) : "")}
          >
            {bound() ? "unbind" : "bind"}
          </button>
        </Show>
      </div>
      <Show
        when={bound()}
        fallback={(
          <input
            class="color-input"
            type="color"
            value={toHex(properties.value, properties.fallback)}
            onInput={event => properties.onChange(fromHex(event.currentTarget.value))}
          />
        )}
      >
        <input
          class="field-input"
          value={typeof properties.value === "string" ? properties.value : ""}
          placeholder="$(hass.home:light.kitchen.color)"
          onInput={event => properties.onChange(event.currentTarget.value)}
        />
      </Show>
    </div>
  );
};

export const NumberField: Component<{
  label: string;
  value: number | null;
  placeholder?: string;
  onChange: (value: string) => void;
}> = properties => (
  <label class="field-label">
    {properties.label}
    <input
      class="field-input"
      type="number"
      value={properties.value ?? ""}
      placeholder={properties.placeholder}
      onInput={event => properties.onChange(event.currentTarget.value)}
    />
  </label>
);

export const SelectField: Component<{
  label: string;
  value: string;
  options: Array<{ value: string; label: string; }>;
  onChange: (value: string) => void;
}> = properties => (
  <label class="field-label">
    {properties.label}
    <select
      class="field-input"
      value={properties.value}
      onInput={event => properties.onChange(event.currentTarget.value)}
    >
      <option value="" />
      <For each={properties.options}>
        {option => <option value={option.value}>{option.label}</option>}
      </For>
    </select>
  </label>
);

/**
 * A field whose options the instance supplies. Backed by a `<datalist>` rather than a `<select>`
 * so you can pick a light by its friendly name, or still type a raw entity id when the instance is
 * offline or the thing you want was never in the list.
 */
export const LookupField: Component<{
  label: string;
  value: string;
  integrationId: string;
  source: string;
  onChange: (value: string) => void;
}> = (properties) => {
  const [options] = createResource(
    () => [properties.integrationId, properties.source] as const,
    ([integrationId, source]) => fetchLookup(integrationId, source),
  );
  const listId = createUniqueId();
  const grouped = () => {
    const byGroup = new Map<string, LookupOption[]>();

    const available = options() ?? [];

    for (const option of available) {
      const group = option.group ?? "";

      byGroup.set(group, [...(byGroup.get(group) ?? []), option]);
    }

    // The daemon returns entities ordered by id, so groups already cluster alphabetically.
    return [...byGroup];
  };

  return (
    <label class="field-label">
      {properties.label}
      <input
        class="field-input"
        value={properties.value}
        list={listId}
        placeholder="light.kitchen"
        onInput={event => properties.onChange(event.currentTarget.value)}
      />
      <datalist id={listId}>
        <For each={grouped()}>
          {([group, entries]) => (
            <For each={entries}>
              {option => (
                <option value={option.value}>
                  {group === "" ? option.label : `${group} - ${option.label}`}
                </option>
              )}
            </For>
          )}
        </For>
      </datalist>
    </label>
  );
};

/**
 * Renders one manifest-declared field. A secret input starts blank on purpose: the daemon never
 * sends a stored credential back, and leaving it blank keeps whatever is already configured.
 */
export const ConfigFieldInput: Component<{
  field: ConfigField;
  value: unknown;
  /** Which instance answers a lookup field. Absent where no field can be a lookup. */
  integrationId?: string;
  onChange: (value: string | boolean) => void;
}> = (properties) => {
  const label = () => (properties.field.is_required ? `${properties.field.label} *` : properties.field.label);
  const text = () => (typeof properties.value === "string" ? properties.value : "");
  const selectOptions = () => (properties.field.kind.type === "select" ? properties.field.kind.options : null);
  const lookupSource = () => (properties.field.kind.type === "lookup" ? properties.field.kind.source : null);

  return (
    <div class="grid gap-1">
      <Switch
        fallback={(
          <TextField
            label={label()}
            value={text()}
            placeholder={properties.field.placeholder ?? undefined}
            onChange={properties.onChange}
          />
        )}
      >
        <Match when={properties.field.kind.type === "number"}>
          <NumberField
            label={label()}
            value={typeof properties.value === "number" ? properties.value : null}
            placeholder={properties.field.placeholder ?? undefined}
            onChange={properties.onChange}
          />
        </Match>
        <Match when={properties.field.kind.type === "boolean"}>
          <label class="check-tile">
            <input
              type="checkbox"
              checked={properties.value === true}
              onInput={event => properties.onChange(event.currentTarget.checked)}
            />
            {properties.field.label}
          </label>
        </Match>
        <Match when={properties.field.kind.type === "secret"}>
          <label class="field-label">
            {label()}
            <input
              class="field-input"
              type="password"
              value={text()}
              placeholder="unchanged"
              autocomplete="off"
              onInput={event => properties.onChange(event.currentTarget.value)}
            />
          </label>
        </Match>
        <Match when={lookupSource()}>
          {source => (
            <LookupField
              label={label()}
              value={text()}
              integrationId={properties.integrationId ?? ""}
              source={source()}
              onChange={properties.onChange}
            />
          )}
        </Match>
        <Match when={selectOptions()}>
          {options => (
            <SelectField
              label={label()}
              value={text()}
              options={options()}
              onChange={properties.onChange}
            />
          )}
        </Match>
      </Switch>
      <Show when={properties.field.help}>{help => <p class="hint">{help()}</p>}</Show>
    </div>
  );
};
