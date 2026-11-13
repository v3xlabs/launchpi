import { Device } from "../api/inventory";

/**
 * A device with children, such as a Network Dock, is how a deck gets on the network rather than a
 * deck itself: one child stands in for it, and several are grouped under its name.
 */
export type DeviceTreeEntry
  = | { kind: "device"; device: Device; via: Device | null; }
    | { kind: "group"; link: Device; children: Device[]; };

export const deviceTree = (devices: Device[]): DeviceTreeEntry[] =>
  devices
    .filter(device => device.parent_surface_id === null)
    .map((root): DeviceTreeEntry => {
      const children = devices.filter(device => device.parent_surface_id === root.surface_id);

      if (children.length === 0) return { kind: "device", device: root, via: null };

      if (children.length === 1) return { kind: "device", device: children[0], via: root };

      return { kind: "group", link: root, children };
    });
