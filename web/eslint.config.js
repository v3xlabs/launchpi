import v3xlabs from "eslint-plugin-v3xlabs";

export default [
  { ignores: ["dist/**"] },
  ...v3xlabs.configs.recommended,
  ...v3xlabs.configs.solid,
  // A flat config has to default-export the array the plugin's own rule forbids everywhere else.
  { files: ["eslint.config.js"], rules: { "import/no-default-export": "off" } },
  {
    // In .tsx the trailing comma of `<T,>` is what stops a type parameter parsing as JSX. It is
    // syntax, not punctuation style, and removing it breaks the file.
    files: ["**/*.tsx"],
    rules: {
      "@stylistic/comma-dangle": [
        "error",
        {
          arrays: "always-multiline",
          objects: "always-multiline",
          imports: "always-multiline",
          exports: "always-multiline",
          functions: "always-multiline",
          enums: "always-multiline",
          tuples: "always-multiline",
          generics: "ignore",
        },
      ],
    },
  },
];
