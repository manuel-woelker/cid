import { createRoute, createRouter } from "@tanstack/react-router";

import { DashboardPage } from "../features/dashboard/dashboard-page";
import { BranchPage } from "../features/repositories/branch-page";
import { RepositoryPage } from "../features/repositories/repository-page";
import { RunDetailPage } from "../features/runs/run-detail-page";
import { rootRoute } from "./root-layout";

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: DashboardPage,
});

const runDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/repositories/$repositoryName/runs/$runId",
  component: RunDetailPage,
});

const repositoryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/repositories/$repositoryName",
  component: RepositoryPage,
});

const branchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/repositories/$repositoryName/branches/$branchName",
  component: BranchPage,
});

export const routeTree = rootRoute.addChildren([
  dashboardRoute,
  runDetailRoute,
  repositoryRoute,
  branchRoute,
]);

export function createAppRouter() {
  return createRouter({
    routeTree,
  });
}

export const router = createAppRouter();

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
