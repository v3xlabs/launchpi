import { Component, createResource, createSignal, For, Match, Show, Switch } from "solid-js";

import { ColorBinding } from "../api/inventory";
import { ConfigField, fetchLookup } from "../api/plugins";
import { useInventory } from "../context/InventoryContext";
import { fromHex, isReference, parseHex, rgbHex, toHex } from "../utils/rendered";
import { interpolateVariables } from "../utils/variables";
import { ReferenceInput } from "./ReferenceInput";
import { optionRows, suggestionKeyDown, SuggestionList, SuggestionRow } from "./SuggestionList";
import { ValueField } from "./ValueField";

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
 * A colour, parametrised like every other field: one input that holds either a hex literal or a
 * `$(instance:value)` reference, with no mode to switch between. Typing `$(` is the switch, and the
 * swatch beside it opens the native picker.
 *
 * The swatch shows what the field will actually paint: for a reference that means the value's
 * current colour, so "make this key the colour of that light" is visible rather than inferred.
 */
export const ColorField: Component<{
  label: string;
  value: ColorBinding | null;
  fallback: string;
  /** Dials take a fixed colour only; binding them is a later phase. */
  bindable?: boolean;
  onChange: (value: ColorBinding) => void;
}> = (properties) => {
  const store = useInventory();
  const text = () => {
    if (properties.value === null) return "";

    return isReference(properties.value)
      ? properties.value
      : toHex(properties.value, properties.fallback);
  };
  /** What the key will actually be painted, resolved the same way the daemon resolves it. */
  const painted = () => parseHex(interpolateVariables(text(), key => store.variables[key]));
  const swatch = () => {
    const color = painted();

    return color === null ? properties.fallback : rgbHex(color);
  };
  /** A hex stays a fixed colour rather than becoming a reference that happens to parse. */
  const commit = (next: string) => {
    const parsed = next.includes("$(") ? null : parseHex(next);

    properties.onChange(parsed ?? next);
  };

  const picker = (
    <input
      class="parameter-swatch"
      type="color"
      aria-label={`${properties.label} colour`}
      value={swatch()}
      data-unresolved={painted() === null}
      onInput={event => properties.onChange(fromHex(event.currentTarget.value))}
    />
  );

  return (
    <div class="grid min-w-0 gap-1">
      <span class="field-label">{properties.label}</span>
      <Show when={properties.bindable !== false} fallback={picker}>
        <ReferenceInput
          value={text()}
          placeholder={properties.fallback}
          leading={picker}
          onChange={commit}
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
 * A field whose options the instance supplies, narrowed by what you type.
 *
 * Not a `<select>`: you can still type a raw entity id when the instance is offline, or a `$(...)`
 * reference to choose the entity from another value.
 */
export const LookupField: Component<{
  label: string;
  value: string;
  integrationId: string;
  source: string;
  onChange: (value: string) => void;
}> = (properties) => {
  const [query, setQuery] = createSignal<string | null>(null);
  const [activeIndex, setActiveIndex] = createSignal(-1);

  const [options] = createResource(
    () => {
      const term = query();

      return term === null
        ? null
        : ([properties.integrationId, properties.source, term] as const);
    },
    ([integrationId, source, term]) => fetchLookup(integrationId, source, term),
  );

  const available = () => options.latest ?? [];

  const close = () => {
    setQuery(null);
    setActiveIndex(-1);
  };

  const choose = (row: SuggestionRow) => {
    if (row.kind !== "option") return;

    properties.onChange(row.option.value);
    close();
  };

  const onKeyDown = suggestionKeyDown({
    count: () => (query() === null ? 0 : available().length),
    activeIndex,
    setActiveIndex,
    accept: () => {
      const option = available()[activeIndex()];

      if (option !== undefined) choose({ kind: "option", option });
    },
    close,
  });

  return (
    <div class="completing-field">
      <label class="field-label">
        {properties.label}
        <input
          class="field-input"
          value={properties.value}
          placeholder="light.kitchen"
          autocomplete="off"
          onFocus={() => setQuery(properties.value)}
          onInput={(event) => {
            properties.onChange(event.currentTarget.value);
            setQuery(event.currentTarget.value);
            setActiveIndex(-1);
          }}
          onKeyDown={onKeyDown}
          onBlur={() => setTimeout(close, 150)}
        />
      </label>
      <Show when={query() !== null}>
        <SuggestionList
          rows={optionRows(available())}
          isLoading={options.loading}
          activeIndex={activeIndex()}
          onChoose={choose}
        />
      </Show>
    </div>
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
  /**
   * Whether a text field here is interpolated before use. True for action parameters, false for
   * plugin configuration, where a `$(...)` would be stored and sent literally.
   */
  supportsReferences?: boolean;
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
          <Show
            when={properties.supportsReferences}
            fallback={(
              <TextField
                label={label()}
                value={text()}
                placeholder={properties.field.placeholder ?? undefined}
                onChange={properties.onChange}
              />
            )}
          >
            <ValueField
              label={label()}
              value={text()}
              placeholder={properties.field.placeholder ?? undefined}
              onChange={properties.onChange}
            />
          </Show>
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
