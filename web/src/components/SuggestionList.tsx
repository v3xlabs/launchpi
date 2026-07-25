import { Component, For, Show } from "solid-js";

import { LookupOption } from "../api/plugins";
import { parseHex } from "../utils/rendered";

/** A value that reads as a colour gets shown as one; anything else is shown as text. */
const swatch = (preview: string | null): string | null => {
  if (preview === null) return null;

  const color = parseHex(preview);

  return color === null ? null : `rgb(${color.red} ${color.green} ${color.blue})`;
};

/** The dropdown shared by every field that completes against the daemon. */
export const SuggestionList: Component<{
  options: LookupOption[] | undefined;
  isLoading: boolean;
  activeIndex: number;
  onChoose: (option: LookupOption) => void;
}> = properties => (
  <div class="suggestions" role="listbox">
    <Show
      when={(properties.options?.length ?? 0) > 0}
      fallback={(
        <p class="suggestion-empty">
          {properties.isLoading ? "Looking..." : "Nothing matches that."}
        </p>
      )}
    >
      <For each={properties.options}>
        {(option, index) => (
          <button
            type="button"
            role="option"
            class="suggestion"
            aria-selected={index() === properties.activeIndex}
            data-active={index() === properties.activeIndex}
            // Pointer-down fires before the input's blur, so the click is not lost to the close.
            onMouseDown={event => event.preventDefault()}
            onClick={() => properties.onChoose(option)}
          >
            <Show when={swatch(option.preview)}>
              {fill => <span class="suggestion-swatch" style={{ background: fill() }} />}
            </Show>
            <span class="min-w-0 flex-1">
              <span class="suggestion-label block">{option.label}</span>
              <span class="suggestion-value block">{option.value}</span>
            </span>
            <Show when={option.preview}>
              {preview => <span class="suggestion-preview">{preview()}</span>}
            </Show>
            <Show when={option.group}>
              {group => <span class="chip chip-muted shrink-0">{group()}</span>}
            </Show>
          </button>
        )}
      </For>
    </Show>
  </div>
);

/** Keyboard handling common to the completing fields: move, accept, dismiss. */
export const suggestionKeyDown = (handlers: {
  count: () => number;
  activeIndex: () => number;
  setActiveIndex: (index: number) => void;
  accept: () => void;
  close: () => void;
}) =>
  (event: KeyboardEvent): void => {
    const count = handlers.count();

    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (count === 0) return;

      event.preventDefault();

      const step = event.key === "ArrowDown" ? 1 : -1;

      handlers.setActiveIndex((handlers.activeIndex() + step + count) % count);

      return;
    }

    if (event.key === "Enter" && handlers.activeIndex() >= 0 && count > 0) {
      event.preventDefault();
      handlers.accept();

      return;
    }

    if (event.key === "Escape") handlers.close();
  };
