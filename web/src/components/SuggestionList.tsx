import { TbFillCaretRight as TbChevron } from "solid-icons/tb";
import { Component, For, Match, Show, Switch } from "solid-js";

import { LookupOption } from "../api/plugins";
import { parseHex } from "../utils/rendered";

/**
 * One row of the dropdown. A group row drills into a plugin rather than completing anything, which
 * is what keeps an installation with thousands of entities browsable instead of merely searchable.
 */
export type SuggestionRow
  = | { kind: "group"; integrationId: string; label: string; detail: string; }
    | { kind: "option"; option: LookupOption; };

export const optionRows = (options: LookupOption[]): SuggestionRow[] =>
  options.map(option => ({ kind: "option", option }));

/** A value that reads as a colour gets shown as one; anything else is shown as text. */
const swatch = (preview: string | null): string | null => {
  if (preview === null) return null;

  const color = parseHex(preview);

  return color === null ? null : `rgb(${color.red} ${color.green} ${color.blue})`;
};

/** The dropdown shared by every field that completes against the daemon. */
export const SuggestionList: Component<{
  rows: SuggestionRow[];
  isLoading: boolean;
  activeIndex: number;
  /** Redundant once a plugin has been drilled into: every row below belongs to it. */
  showGroup?: boolean;
  /** Present while drilled in, so there is a way back that is not just Escape. */
  scope?: { label: string; onBack: () => void; };
  onChoose: (row: SuggestionRow) => void;
}> = properties => (
  <div class="suggestions" role="listbox">
    <Show when={properties.scope}>
      {scope => (
        <button
          type="button"
          class="suggestion-scope"
          onMouseDown={event => event.preventDefault()}
          onClick={scope().onBack}
        >
          <span class="suggestion-back">&lt;</span>
          {scope().label}
        </button>
      )}
    </Show>
    <Show
      when={properties.rows.length > 0}
      fallback={(
        <p class="suggestion-empty">
          {properties.isLoading ? "Looking..." : "Nothing matches that."}
        </p>
      )}
    >
      <For each={properties.rows}>
        {(row, index) => (
          <button
            type="button"
            role="option"
            class="suggestion"
            aria-selected={index() === properties.activeIndex}
            data-active={index() === properties.activeIndex}
            // Pointer-down fires before the input's blur, so the click is not lost to the close.
            onMouseDown={event => event.preventDefault()}
            onClick={() => properties.onChoose(row)}
          >
            <Switch>
              <Match when={row.kind === "group" ? row : null}>
                {group => (
                  <>
                    <span class="min-w-0 flex-1">
                      <span class="suggestion-label block">{group().label}</span>
                      <Show when={group().detail !== group().label}>
                        <span class="suggestion-value block">{group().detail}</span>
                      </Show>
                    </span>
                    <TbChevron class="h-3 w-3 shrink-0 text-neutral-500" />
                  </>
                )}
              </Match>
              <Match when={row.kind === "option" ? row.option : null}>
                {option => (
                  <>
                    <Show when={swatch(option().preview)}>
                      {fill => <span class="suggestion-swatch" style={{ background: fill() }} />}
                    </Show>
                    <span class="min-w-0 flex-1">
                      <span class="suggestion-label block">{option().label}</span>
                      <span class="suggestion-value block">{option().value}</span>
                    </span>
                    <Show when={option().preview}>
                      {preview => <span class="suggestion-preview">{preview()}</span>}
                    </Show>
                    <Show when={properties.showGroup !== false && option().group}>
                      {group => <span class="chip chip-muted shrink-0">{group()}</span>}
                    </Show>
                  </>
                )}
              </Match>
            </Switch>
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
