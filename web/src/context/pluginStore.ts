import { Accessor, createSignal, onMount } from "solid-js";
import { createStore } from "solid-js/store";

import * as pluginApi from "../api/plugins";
import { forgetRendersUsing } from "../api/render";

export type PluginStore = {
  plugins: Accessor<pluginApi.PluginCatalogue>;
  refreshPlugins: () => Promise<void>;
  createPluginInstance: (input: pluginApi.CreateInstanceInput) => Promise<boolean>;
  updatePluginInstance: (
    integrationId: string,
    input: pluginApi.UpdateInstanceInput,
  ) => Promise<boolean>;
  deletePluginInstance: (integrationId: string) => Promise<boolean>;
  runPluginAction: (
    integrationId: string,
    actionName: string,
    parameters: Record<string, unknown>,
  ) => Promise<boolean>;
  /** Live values keyed by `integration_id:name`, patched from the event stream. */
  variables: Record<string, string>;
  setVariable: (integrationId: string, name: string, rendered: string) => void;
  copyToClipboard: (load: () => Promise<string>) => Promise<boolean>;
  /** Arrival count per asset, so a key redraws only when its own picture lands. */
  assetArrivals: Record<string, number>;
  assetReady: (asset: string) => void;
  values: Accessor<pluginApi.ValueCatalogue>;
  refreshValues: () => Promise<void>;
  saveUserValue: (value: pluginApi.UserValue) => Promise<boolean>;
  removeUserValue: (name: string) => Promise<boolean>;
};

/**
 * The plugin half of the inventory store. Split out so the provider stays readable; `run` is the
 * provider's shared saving/error wrapper, reused here so both halves report failures the same way.
 */
export const createPluginStore = (
  run: (operation: () => Promise<void>) => Promise<boolean>,
  setError: (message: string | null) => void,
): PluginStore => {
  const [catalogue, setCatalogue] = createSignal<pluginApi.PluginCatalogue>(pluginApi.emptyCatalogue);
  const [values, setValues] = createSignal<pluginApi.ValueCatalogue>(pluginApi.emptyValueCatalogue);
  const [variables, setVariables] = createStore<Record<string, string>>({});
  const [assetArrivals, setAssetArrivals] = createStore<Record<string, number>>({});

  const refreshValues = async (): Promise<void> => {
    try {
      const catalogue = await pluginApi.fetchValues();

      setValues(catalogue);

      // Seed the live map too. It is otherwise filled only by change events, so a freshly loaded
      // page would resolve every `$(...)` to nothing until something happened to move.
      for (const entry of catalogue.values) {
        setVariables(`${entry.integration_id}:${entry.name}`, entry.rendered);
      }
    }
    catch (valuesError) {
      setError(valuesError instanceof Error ? valuesError.message : "Unable to load values.");
    }
  };

  const refreshPlugins = async (): Promise<void> => {
    try {
      setCatalogue(await pluginApi.fetchPlugins());
      await refreshValues();
    }
    catch (pluginError) {
      setError(pluginError instanceof Error ? pluginError.message : "Unable to load plugins.");
    }
  };

  // Every plugin mutation restarts the instance, so the catalogue is always refetched after.
  const runPlugin = async (operation: () => Promise<void>): Promise<boolean> => {
    const succeeded = await run(operation);

    await refreshPlugins();

    return succeeded;
  };

  onMount(() => {
    void refreshPlugins();
  });

  return {
    plugins: catalogue,
    refreshPlugins,
    createPluginInstance: input =>
      runPlugin(async () => {
        await pluginApi.createInstance(input);
      }),
    updatePluginInstance: (integrationId, input) =>
      runPlugin(async () => {
        await pluginApi.updateInstance(integrationId, input);
      }),
    deletePluginInstance: integrationId =>
      runPlugin(async () => {
        await pluginApi.deleteInstance(integrationId);
      }),
    runPluginAction: (integrationId, actionName, parameters) =>
      runPlugin(async () => {
        await pluginApi.runAction(integrationId, actionName, parameters);
      }),
    variables,
    setVariable: (integrationId, name, rendered) =>
      setVariables(`${integrationId}:${name}`, rendered),
    assetArrivals,
    assetReady: (asset) => {
      forgetRendersUsing();
      setAssetArrivals(asset, arrivals => (arrivals ?? 0) + 1);
    },
    values,
    refreshValues,
    saveUserValue: value =>
      runPlugin(async () => {
        await pluginApi.upsertUserValue(value);
      }),
    removeUserValue: name =>
      runPlugin(async () => {
        await pluginApi.deleteUserValue(name);
      }),
    copyToClipboard: async (load) => {
      try {
        await navigator.clipboard.writeText(await load());

        return true;
      }
      catch (copyError) {
        setError(copyError instanceof Error ? copyError.message : "Unable to copy configuration.");

        return false;
      }
    },
  };
};
