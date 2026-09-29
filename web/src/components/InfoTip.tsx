import * as Tooltip from "@kobalte/core/tooltip";
import { FiInfo } from "solid-icons/fi";
import { Component, createSignal, JSX } from "solid-js";

/** Explanations the reader needs once live here, behind a glyph, instead of under every field. */
export const InfoTip: Component<{ children: JSX.Element; label?: string; }> = (properties) => {
  const [isOpen, setIsOpen] = createSignal(false);
  let trigger: HTMLSpanElement | undefined;

  return (
    <Tooltip.Root
      openDelay={150}
      open={isOpen()}
      // A dialog or popover focuses its first tabbable element on open, which is often this glyph.
      // That focus comes from a click, so it is not focus-visible and must not pop the tooltip.
      onOpenChange={isNextOpen => setIsOpen(isNextOpen && trigger?.matches(":hover, :focus-visible") === true)}
    >
      <Tooltip.Trigger
        as="span"
        ref={element => (trigger = element)}
        tabIndex={0}
        class="inline-grid size-4 shrink-0 cursor-help place-items-center rounded-full text-muted"
        aria-label={properties.label ?? "More information"}
      >
        <FiInfo class="size-3.5" />
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content class="tooltip">{properties.children}</Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
};

/** Any element with a short inverted tooltip, for glyph buttons whose label is not visible. */
export const Tip: Component<{ label: string; children: JSX.Element; }> = properties => (
  <Tooltip.Root openDelay={300}>
    <Tooltip.Trigger as="span" class="inline-flex">{properties.children}</Tooltip.Trigger>
    <Tooltip.Portal>
      <Tooltip.Content class="tooltip">{properties.label}</Tooltip.Content>
    </Tooltip.Portal>
  </Tooltip.Root>
);
