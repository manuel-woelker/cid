import {
  createRoute,
  createRouter,
} from "@tanstack/react-router";

import { DashboardPage } from "../features/dashboard/dashboard-page";
import { RunDetailPage } from "../features/runs/run-detail-page";
import { rootRoute } from "./root-layout";

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: DashboardPage,
});

const runDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runs/$runId",
  component: RunDetailPage,
});

const routeTree = rootRoute.addChildren([dashboardRoute, runDetailRoute]);

export const router = createRouter({
  routeTree,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
