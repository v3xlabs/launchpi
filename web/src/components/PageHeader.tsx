import { Component, JSX, Show } from "solid-js";

/** The one page header: a 40px mark, the title, one line of facts, and the page's actions. */
export const PageHeader: Component<{
  mark: JSX.Element;
  title: JSX.Element;
  meta?: JSX.Element;
  actions?: JSX.Element;
}> = properties => (
  <div class="flex items-center gap-3">
    <div class="grid size-10 shrink-0 place-items-center overflow-hidden rounded-control bg-raised text-soft">
      {properties.mark}
    </div>
    <div class="min-w-0 flex-1">
      <h1 class="flex min-w-0 items-center gap-2 text-lg font-semibold">{properties.title}</h1>
      <Show when={properties.meta}>
        <p class="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-2 text-muted">{properties.meta}</p>
      </Show>
    </div>
    <Show when={properties.actions}>
      <div class="flex shrink-0 items-center gap-2">{properties.actions}</div>
    </Show>
  </div>
);

export const MetaSeparator: Component = () => (
  <span class="mx-1 inline-block size-1 shrink-0 rounded-full bg-slate-300 align-middle dark:bg-slate-600" aria-hidden="true" />
);
