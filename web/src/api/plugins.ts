import { fetchText, isBoolean, isNumber, isRecord, isString, request } from "./guards";
// Type-only, so this does not create a runtime cycle with inventory.ts importing PluginInstance.
import type { DeviceStatus } from "./inventory";

export type ConfigFieldKind
  = | { type: "text"; }
    | { type: "number"; }
    | { type: "boolean"; }
    | { type: "secret"; }
    | { type: "select"; options: Array<{ value: string; label: string; }>; }
    | { type: "lookup"; source: string; };
export type ConfigField = {
  key: string;
  label: string;
  kind: ConfigFieldKind;
  is_required: boolean;
  placeholder: string | null;
  help: string | null;
};
export type ActionDefinition = {
  name: string;
  label: string;
  description: string | null;
  parameters: ConfigField[];
};
export type VariableDefinition = {
  name: string;
  label: string;
  description: string | null;
  kind: "text" | "number" | "boolean" | "image";
};
export type PluginManifest = {
  plugin_type: string;
  display_name: string;
  description: string;
  config_schema: ConfigField[];
  actions: ActionDefinition[];
  variables: VariableDefinition[];
};
export type PluginInstanceStatus
  = | { state: "starting"; }
    | { state: "running"; }
    | { state: "disabled"; }
    | { state: "error"; reason: string; };
export type PluginInstance = {
  integration_id: string;
  plugin_type: string;
  name: string;
  display_name: string;
  is_enabled: boolean;
  status: PluginInstanceStatus;
  /** Current configuration with declared secrets removed by the daemon. */
  config: Record<string, unknown>;
};
export type PluginCatalogue = { types: PluginManifest[]; instances: PluginInstance[]; };
export type VariableEntry = {
  integration_id: string;
  name: string;
  value: unknown;
  rendered: string;
};

const isConfigFieldKind = (value: unknown): value is ConfigFieldKind =>
  isRecord(value)
  && isString(value.type)
  && ["text", "number", "boolean", "secret", "select", "lookup"].includes(value.type);
const isConfigField = (value: unknown): value is ConfigField =>
  isRecord(value)
  && isString(value.key)
  && isString(value.label)
  && isConfigFieldKind(value.kind)
  && isBoolean(value.is_required);
const isDefinition = (value: unknown): value is ActionDefinition =>
  isRecord(value)
  && isString(value.name)
  && isString(value.label)
  && Array.isArray(value.parameters)
  && value.parameters.every(isConfigField);
const isVariableDefinition = (value: unknown): value is VariableDefinition =>
  isRecord(value) && isString(value.name) && isString(value.label) && isString(value.kind);
const isManifest = (value: unknown): value is PluginManifest =>
  isRecord(value)
  && isString(value.plugin_type)
  && isString(value.display_name)
  && Array.isArray(value.config_schema)
  && value.config_schema.every(isConfigField)
  && Array.isArray(value.actions)
  && value.actions.every(isDefinition)
  && Array.isArray(value.variables)
  && value.variables.every(isVariableDefinition);

export const isPluginInstance = (value: unknown): value is PluginInstance =>
  isRecord(value)
  && isString(value.integration_id)
  && isString(value.plugin_type)
  && isString(value.name)
  && isString(value.display_name)
  && isBoolean(value.is_enabled)
  && isRecord(value.status)
  && isString(value.status.state)
  && (value.config === undefined || isRecord(value.config));

const isVariableEntry = (value: unknown): value is VariableEntry =>
  isRecord(value) && isString(value.integration_id) && isString(value.name) && isString(value.rendered);

export const statusLabel = (status: PluginInstanceStatus): string => {
  switch (status.state) {
    case "running": {
      return "Running";
    }
    case "starting": {
      return "Starting";
    }
    case "disabled": {
      return "Disabled";
    }
    default: {
      return "Error";
    }
  }
};
/** Reuses the device status vocabulary so one StatusDot serves both pages. */
export const statusTone = (status: PluginInstanceStatus): DeviceStatus => {
  switch (status.state) {
    case "running": {
      return "connected";
    }
    case "starting": {
      return "connecting";
    }
    case "disabled": {
      return "disabled";
    }
    default: {
      return "unavailable";
    }
  }
};
export const statusReason = (status: PluginInstanceStatus): string | null =>
  (status.state === "error" ? status.reason : null);

export const variableReference = (integrationId: string, name: string): string =>
  `$(${integrationId}:${name})`;

export const emptyCatalogue: PluginCatalogue = { types: [], instances: [] };

export const fetchPlugins = async (): Promise<PluginCatalogue> => {
  const response = await fetch("/api/plugins");

  if (!response.ok) throw new Error(`Request failed with status ${response.status}`);

  const data: unknown = await response.json();

  if (
    !isRecord(data)
    || !Array.isArray(data.types)
    || !data.types.every(isManifest)
    || !Array.isArray(data.instances)
    || !data.instances.every(isPluginInstance)
  ) {
    throw new Error("The daemon returned an invalid plugin catalogue.");
  }

  return { types: data.types, instances: data.instances };
};

export const fetchVariables = async (integrationId: string): Promise<VariableEntry[]> => {
  const response = await fetch(`/api/plugins/${encodeURIComponent(integrationId)}/variables`);

  if (!response.ok) throw new Error(`Request failed with status ${response.status}`);

  const data: unknown = await response.json();

  if (!Array.isArray(data) || !data.every(isVariableEntry)) {
    throw new Error("The daemon returned invalid variables.");
  }

  return data;
};

export type LookupOption = { value: string; label: string; group: string | null; };

const isLookupOption = (value: unknown): value is LookupOption =>
  isRecord(value) && isString(value.value) && isString(value.label);

const readOptions = async (response: Response): Promise<LookupOption[]> => {
  if (!response.ok) return [];

  const data: unknown = await response.json();

  return Array.isArray(data) && data.every(isLookupOption) ? data : [];
};

/** Options for a lookup field. An instance that is down simply offers none. */
export const fetchLookup = async (
  integrationId: string,
  source: string,
  query: string,
): Promise<LookupOption[]> =>
  readOptions(
    await fetch(
      `/api/plugins/${encodeURIComponent(integrationId)}/lookup/${
        encodeURIComponent(source)
      }?q=${encodeURIComponent(query)}`,
    ),
  );

/**
 * Every reference a field could hold, narrowed by what has been typed. Live values and what the
 * running plugins say they *could* publish are merged by the daemon, so a light that has never been
 * read is still offered.
 */
export const fetchSuggestions = async (query: string): Promise<LookupOption[]> =>
  readOptions(await fetch(`/api/values/suggest?q=${encodeURIComponent(query)}`));

export type UserValue = { name: string; value: unknown; description: string | null; };
export type AvailableAction = {
  integration_id: string;
  instance_name: string;
  name: string;
  label: string;
  description: string | null;
  parameters: ConfigField[];
};
export type ValueCatalogue = {
  values: VariableEntry[];
  user_values: UserValue[];
  actions: AvailableAction[];
};

export const emptyValueCatalogue: ValueCatalogue = { values: [], user_values: [], actions: [] };

const isUserValue = (value: unknown): value is UserValue =>
  isRecord(value) && isString(value.name);
const isAvailableAction = (value: unknown): value is AvailableAction =>
  isRecord(value)
  && isString(value.integration_id)
  && isString(value.name)
  && isString(value.label)
  && Array.isArray(value.parameters)
  && value.parameters.every(isConfigField);

export const fetchValues = async (): Promise<ValueCatalogue> => {
  const response = await fetch("/api/values");

  if (!response.ok) throw new Error(`Request failed with status ${response.status}`);

  const data: unknown = await response.json();

  if (
    !isRecord(data)
    || !Array.isArray(data.values)
    || !data.values.every(isVariableEntry)
    || !Array.isArray(data.user_values)
    || !data.user_values.every(isUserValue)
    || !Array.isArray(data.actions)
    || !data.actions.every(isAvailableAction)
  ) {
    throw new Error("The daemon returned an invalid value catalogue.");
  }

  return { values: data.values, user_values: data.user_values, actions: data.actions };
};

export const upsertUserValue = (value: UserValue): Promise<Response> =>
  request("/api/values", "POST", value);
export const deleteUserValue = (name: string): Promise<Response> =>
  request(`/api/values/${encodeURIComponent(name)}`, "DELETE");

/**
 * TOML distinguishes a number from the string "1", and a value typed into a text box arrives as a
 * string either way. Guessing from the shape is what lets a user write `12` and get a number.
 */
export const parseUserValue = (raw: string): unknown => {
  const trimmed = raw.trim();

  if (trimmed === "true") return true;

  if (trimmed === "false") return false;

  if (trimmed !== "" && !Number.isNaN(Number(trimmed))) return Number(trimmed);

  return raw;
};

export type CreateInstanceInput = {
  plugin_type: string;
  name: string;
  display_name: string | null;
  config: Record<string, unknown>;
};
export type UpdateInstanceInput = {
  is_enabled?: boolean;
  display_name?: string;
  config?: Record<string, unknown>;
};

export const createInstance = (input: CreateInstanceInput): Promise<Response> =>
  request("/api/plugins", "POST", input);
export const updateInstance = (integrationId: string, input: UpdateInstanceInput): Promise<Response> =>
  request(`/api/plugins/${encodeURIComponent(integrationId)}`, "PATCH", input);
export const deleteInstance = (integrationId: string): Promise<Response> =>
  request(`/api/plugins/${encodeURIComponent(integrationId)}`, "DELETE");
export const runAction = (
  integrationId: string,
  actionName: string,
  parameters: Record<string, unknown>,
): Promise<Response> =>
  request(
    `/api/plugins/${encodeURIComponent(integrationId)}/actions/${encodeURIComponent(actionName)}`,
    "POST",
    parameters,
  );
export const fetchInstanceConfig = (integrationId: string): Promise<string> =>
  fetchText(`/api/plugins/${encodeURIComponent(integrationId)}/config`);

/**
 * A secret is never sent back to the browser, so a form starts blank. Sending the blank back would
 * clear a stored credential, which is why untouched secrets are dropped from the payload instead.
 */
export const withoutUntouchedSecrets = (
  schema: ConfigField[],
  values: Record<string, unknown>,
): Record<string, unknown> => {
  const secrets = new Set(schema.filter(field => field.kind.type === "secret").map(field => field.key));
  const blanks = new Set<unknown>(["", null, undefined]);

  return Object.fromEntries(
    Object.entries(values).filter(([key, value]) => !(secrets.has(key) && blanks.has(value))),
  );
};

/** Number fields arrive from an <input> as strings; TOML needs them typed. */
export const coerceConfigValue = (field: ConfigField, raw: string | boolean): unknown => {
  if (typeof raw === "boolean") return raw;

  if (raw === "") return null;

  if (field.kind.type === "number") {
    const parsed = Number(raw);

    return isNumber(parsed) ? parsed : null;
  }

  return raw;
};
