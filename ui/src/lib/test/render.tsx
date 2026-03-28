import { render } from "@testing-library/react";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";

import { createAppRouter } from "../../app/router";

export async function renderApp(initialPath = "/") {
  const router = createAppRouter();
  const history = createMemoryHistory({
    initialEntries: [initialPath],
  });

  router.update({
    history,
  });

  await router.load();

  return render(<RouterProvider router={router} />);
}
