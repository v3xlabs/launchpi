import { getErrorMessage, isNumber, isRecord, isString } from "./guards";

/** `declarative` means the configuration directory is read-only: every edit lasts until restart. */
export type ConfigState = {
  mode: "declarative" | "writable";
  discovery: boolean;
  changes: number;
};

export const isConfigState = (value: unknown): value is ConfigState =>
  isRecord(value)
  && (value.mode === "declarative" || value.mode === "writable")
  && typeof value.discovery === "boolean"
  && isNumber(value.changes);

export type ConfigChange = {
  kind: "device" | "panel" | "plugin" | "value";
  entry_id: string;
  name: string;
  change: "added" | "changed" | "removed";
};
export type ConfigChanges = { changes: ConfigChange[]; toml: string; nix: string; };

const changeKinds = new Set<unknown>(["device", "panel", "plugin", "value"]);
const changeWords = new Set<unknown>(["added", "changed", "removed"]);

const isConfigChange = (value: unknown): value is ConfigChange =>
  isRecord(value)
  && changeKinds.has(value.kind)
  && isString(value.entry_id)
  && isString(value.name)
  && changeWords.has(value.change);

export const fetchConfigChanges = async (): Promise<ConfigChanges> => {
  const response = await fetch("/api/config/changes");

  if (!response.ok) throw new Error(await getErrorMessage(response));

  const data: unknown = await response.json();

  if (
    !isRecord(data)
    || !Array.isArray(data.changes)
    || !data.changes.every(isConfigChange)
    || !isString(data.toml)
    || !isString(data.nix)
  ) {
    throw new Error("The daemon returned an invalid change list.");
  }

  return { changes: data.changes, toml: data.toml, nix: data.nix };
};
