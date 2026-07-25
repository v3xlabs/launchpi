import { Component, Show } from "solid-js";

// Product renders live in web/public/images/devices/webp as <slug>-{200,400,full}.webp.
const slugMatchers: Array<{ pattern: RegExp; slug: string; }> = [
  { pattern: /dock/i, slug: "sd-network-dock" },
  { pattern: /launchpad\s*pro/i, slug: "launchpad-pro-mk3" },
  { pattern: /launchpad\s*mini.*mk\.?1/i, slug: "launchpad-mini-mk1" },
  { pattern: /launchpad\s*mini/i, slug: "launchpad-mini-mk3" },
  { pattern: /launchpad\s*x/i, slug: "launchpad-x" },
  { pattern: /studio/i, slug: "sd-studio" },
  { pattern: /\bxl\b/i, slug: "sd-xl" },
  { pattern: /plus/i, slug: "sd-plus" },
  { pattern: /mini/i, slug: "sd-mini" },
  { pattern: /neo/i, slug: "sd-neo" },
  { pattern: /pedal/i, slug: "sd-pedal" },
  { pattern: /mk\.?2/i, slug: "sd-mk2" },
  { pattern: /stream deck/i, slug: "sd-mk2" },
];

const deviceImageSlug = (model: string): string | null =>
  slugMatchers.find(matcher => matcher.pattern.test(model))?.slug ?? null;

type DeviceImageProperties = { model: string; class?: string; };

export const DeviceImage: Component<DeviceImageProperties> = properties => (
  <Show when={deviceImageSlug(properties.model)}>
    {slug => (
      <img
        src={`/images/devices/webp/${slug()}-200.webp`}
        srcset={`/images/devices/webp/${slug()}-200.webp 200w, /images/devices/webp/${slug()}-400.webp 400w`}
        alt=""
        classList={{ "device-image": true, [properties.class ?? ""]: true }}
        loading="lazy"
      />
    )}
  </Show>
);
