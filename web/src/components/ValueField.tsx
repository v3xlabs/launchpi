import { Component } from "solid-js";

import { ReferenceInput } from "./ReferenceInput";

/** A labelled parametrised field: free text that may hold a `$(instance:value)` reference. */
export const ValueField: Component<{
  label: string;
  value: string;
  placeholder?: string;
  onChange: (value: string) => void;
}> = properties => (
  <label class="field-label">
    {properties.label}
    <ReferenceInput
      value={properties.value}
      placeholder={properties.placeholder}
      onChange={properties.onChange}
    />
  </label>
);
