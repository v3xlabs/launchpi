import { Component, createResource, createSignal, JSX, Show } from "solid-js";

import { fetchSuggestions, LookupOption } from "../api/plugins";
import { suggestionKeyDown, SuggestionList } from "./SuggestionList";

/**
 * The caret sits inside an unclosed `$(`: what has been typed of the reference so far.
 *
 * This is what lets suggestions narrow as you type rather than only when the button is pressed, and
 * it is why the field tracks the caret and not just the value.
 */
const openReference = (value: string, caret: number): { start: number; typed: string; } | null => {
  const before = value.slice(0, caret);
  const start = before.lastIndexOf("$(");

  if (start === -1) return null;

  const inside = before.slice(start + 2);

  // A closing paren, or a second opening one, means the caret is past that reference not inside it.
  return inside.includes(")") || inside.includes("$(") ? null : { start, typed: inside };
};

export const isReference = (value: string): boolean => value.includes("$(");

/**
 * A text input that knows its contents may reference a value, and the picker behind it.
 *
 * One control rather than a literal-or-binding mode switch: typing `$(` *is* the switch. Every
 * parametrised field in the editor is this input with different decoration, so completion, the
 * keyboard handling and the value previews behave identically wherever they appear.
 *
 * Suggestions come from the daemon, which asks every running plugin what it *could* publish rather
 * than only what it already has, so a Home Assistant light appears the moment the instance
 * connects, without anything having subscribed to it first.
 */
export const ReferenceInput: Component<{
  value: string;
  placeholder?: string;
  /** Rendered before the input: the colour swatch, where there is one. */
  leading?: JSX.Element;
  onChange: (value: string) => void;
}> = (properties) => {
  const [query, setQuery] = createSignal<string | null>(null);
  const [caret, setCaret] = createSignal(0);
  const [activeIndex, setActiveIndex] = createSignal(-1);
  let input: HTMLInputElement | undefined;
  const holdInput = (element: HTMLInputElement) => (input = element);

  const [suggestions] = createResource(query, term => fetchSuggestions(term));

  const options = () => suggestions.latest ?? [];

  const close = () => {
    setQuery(null);
    setActiveIndex(-1);
  };

  const openPicker = () => {
    setCaret(input?.selectionStart ?? properties.value.length);
    setQuery(openReference(properties.value, input?.selectionStart ?? 0)?.typed ?? "");
    setActiveIndex(-1);
    input?.focus();
  };

  const onInput = (event: InputEvent & { currentTarget: HTMLInputElement; }) => {
    const next = event.currentTarget.value;
    const position = event.currentTarget.selectionStart ?? next.length;
    const open = openReference(next, position);

    properties.onChange(next);
    setCaret(position);
    setActiveIndex(-1);
    setQuery(open === null ? null : open.typed);
  };

  /** Completes the reference being typed, or inserts a whole one at the caret. */
  const choose = (option: LookupOption) => {
    const position = caret();
    const open = openReference(properties.value, position);
    const head = properties.value.slice(0, open === null ? position : open.start);
    const tail = properties.value.slice(position);

    properties.onChange(`${head}${option.value}${tail}`);
    close();
    input?.focus();
  };

  const onKeyDown = suggestionKeyDown({
    count: () => (query() === null ? 0 : options().length),
    activeIndex,
    setActiveIndex,
    accept: () => {
      const option = options()[activeIndex()];

      if (option !== undefined) choose(option);
    },
    close,
  });

  return (
    <div class="completing-field">
      <div class="parameter-input" data-bound={isReference(properties.value)}>
        {properties.leading}
        <input
          ref={holdInput}
          class="parameter-text"
          value={properties.value}
          placeholder={properties.placeholder}
          autocomplete="off"
          onInput={onInput}
          onKeyDown={onKeyDown}
          // Deferred so a click on a suggestion is not cancelled by the field losing focus.
          onBlur={() => setTimeout(close, 150)}
        />
        <button
          type="button"
          class="parameter-pick"
          aria-label="Insert a value"
          title="Insert a value"
          onMouseDown={event => event.preventDefault()}
          onClick={openPicker}
        >
          $()
        </button>
      </div>

      <Show when={query() !== null}>
        <SuggestionList
          options={options()}
          isLoading={suggestions.loading}
          activeIndex={activeIndex()}
          onChoose={choose}
        />
      </Show>
    </div>
  );
};
