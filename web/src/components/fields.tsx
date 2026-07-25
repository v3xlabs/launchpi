import { Component, For, Match, Show, Switch } from "solid-js";

import { RgbaColor } from "../api/inventory";
import { ConfigField } from "../api/plugins";
import { fromHex, toHex } from "../utils/rendered";

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

export const ColorField: Component<{
  label: string;
  value: RgbaColor | null;
  fallback: string;
  onChange: (color: RgbaColor) => void;
}> = properties => (
  <label class="field-label">
    {properties.label}
    <input
      class="color-input"
      type="color"
      value={toHex(properties.value, properties.fallback)}
      onInput={event => properties.onChange(fromHex(event.currentTarget.value))}
    />
  </label>
);

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
 * Renders one manifest-declared field. A secret input starts blank on purpose: the daemon never
 * sends a stored credential back, and leaving it blank keeps whatever is already configured.
 */
export const ConfigFieldInput: Component<{
  field: ConfigField;
  value: unknown;
  onChange: (value: string | boolean) => void;
}> = (properties) => {
  const label = () => (properties.field.is_required ? `${properties.field.label} *` : properties.field.label);
  const text = () => (typeof properties.value === "string" ? properties.value : "");
  const selectOptions = () => (properties.field.kind.type === "select" ? properties.field.kind.options : null);

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
