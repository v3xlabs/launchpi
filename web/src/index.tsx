import { RouterProvider } from "@tanstack/solid-router";
import { render } from "solid-js/web";

import { InventoryProvider } from "./context/InventoryContext";
import { Document } from "./Document";
import { router } from "./router";

const root = document.querySelector("#root");

if (root !== null) {
  render(
    () => (
      <Document>
        <InventoryProvider>
          <RouterProvider router={router} />
        </InventoryProvider>
      </Document>
    ),
    root,
  );
}
