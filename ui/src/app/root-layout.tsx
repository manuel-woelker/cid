import { Breadcrumb, Layout, Space, Typography } from "antd";
import { useEffect, useMemo, useState } from "react";
import {
  Link,
  Outlet,
  createRootRoute,
  useRouterState,
} from "@tanstack/react-router";

import faviconUrl from "../assets/cid-favicon.png";
import { getRepositories, getRepositoryBranches } from "../lib/api/client";
import type { BranchSummary, Repository } from "../lib/api/types";
import { AppProviders } from "./providers";

const navItems = [{ to: "/", label: "Dashboard" }] as const;

interface NavigationState {
  repositories: Repository[];
  activeBranches: BranchSummary[];
}

function decodePathSegment(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function breadcrumbItems(pathname: string) {
  const segments = pathname.split("/").filter(Boolean);

  if (segments.length === 0) {
    return [{ title: "Dashboard" }];
  }

  if (segments[0] !== "repositories") {
    return [{ title: "Dashboard", href: "/" }];
  }

  const items: Array<{ title: string; href?: string }> = [
    { title: "Dashboard", href: "/" },
  ];
  const repositoryId = segments[1];

  if (!repositoryId) {
    items.push({ title: "Repositories" });
    return items;
  }

  items.push({
    title: `Repository ${repositoryId}`,
    href: `/repositories/${repositoryId}`,
  });

  if (segments[2] === "branches" && segments[3]) {
    items.push({
      title: decodePathSegment(segments[3]),
    });
  }

  if (segments[2] === "runs" && segments[3]) {
    items.push({
      title: `Run #${segments[3]}`,
    });
  }

  return items;
}

function RootLayout() {
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
  const [navigationState, setNavigationState] = useState<NavigationState>({
    repositories: [],
    activeBranches: [],
  });
  const activeRepositoryId = useMemo(() => {
    const segments = pathname.split("/").filter(Boolean);
    return segments[0] === "repositories" ? (segments[1] ?? null) : null;
  }, [pathname]);

  useEffect(() => {
    let isMounted = true;

    async function loadRepositories() {
      try {
        const repositories = await getRepositories();
        if (!isMounted) {
          return;
        }

        setNavigationState((currentState) => ({
          ...currentState,
          repositories,
        }));
      } catch {
        if (!isMounted) {
          return;
        }

        setNavigationState((currentState) => ({
          ...currentState,
          repositories: [],
        }));
      }
    }

    void loadRepositories();

    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    if (!activeRepositoryId) {
      setNavigationState((currentState) => ({
        ...currentState,
        activeBranches: [],
      }));
      return;
    }

    const repositoryId = activeRepositoryId;
    let isMounted = true;

    async function loadBranches() {
      try {
        const activeBranches = await getRepositoryBranches(repositoryId);
        if (!isMounted) {
          return;
        }

        setNavigationState((currentState) => ({
          ...currentState,
          activeBranches,
        }));
      } catch {
        if (!isMounted) {
          return;
        }

        setNavigationState((currentState) => ({
          ...currentState,
          activeBranches: [],
        }));
      }
    }

    void loadBranches();

    return () => {
      isMounted = false;
    };
  }, [activeRepositoryId]);

  return (
    <AppProviders>
      <Layout className="app-layout">
        <Layout.Header className="app-header">
          <div className="app-header-main">
            <div className="app-brand">
              <img src={faviconUrl} alt="cid" className="app-logo" />
              <div className="app-brand-copy">
                <Typography.Title level={4}>cid</Typography.Title>
                <Breadcrumb
                  className="app-breadcrumb"
                  items={breadcrumbItems(pathname).map((item) => ({
                    title: item.href ? (
                      <Link to={item.href}>{item.title}</Link>
                    ) : (
                      item.title
                    ),
                  }))}
                />
              </div>
            </div>
          </div>
          <Space size="small">
            {navItems.map((item) => (
              <Link
                key={item.to}
                to={item.to}
                className="app-nav-link"
                activeProps={{ className: "app-nav-link app-nav-link-active" }}
              >
                {item.label}
              </Link>
            ))}
          </Space>
        </Layout.Header>
        <Layout.Content className="app-content">
          <div className="app-shell">
            <aside className="app-sidebar">
              <Typography.Text className="app-sidebar-label">
                Navigation
              </Typography.Text>
              <Link
                to="/"
                className={
                  pathname === "/"
                    ? "app-tree-link app-tree-link-active"
                    : "app-tree-link"
                }
              >
                Dashboard
              </Link>
              <Typography.Text className="app-sidebar-label">
                Repositories
              </Typography.Text>
              <nav className="app-tree">
                {navigationState.repositories.map((repository) => {
                  const isActiveRepository =
                    String(repository.id) === activeRepositoryId;

                  return (
                    <div key={repository.id} className="app-tree-group">
                      <Link
                        to="/repositories/$repositoryId"
                        params={{ repositoryId: String(repository.id) }}
                        className={
                          isActiveRepository
                            ? "app-tree-link app-tree-link-active"
                            : "app-tree-link"
                        }
                      >
                        {repository.name}
                      </Link>
                      {isActiveRepository ? (
                        <div className="app-tree-children">
                          {navigationState.activeBranches.map((branch) => (
                            <Link
                              key={branch.branch_name}
                              to="/repositories/$repositoryId/branches/$branchName"
                              params={{
                                repositoryId: String(repository.id),
                                branchName: branch.branch_name,
                              }}
                              className={
                                pathname ===
                                `/repositories/${repository.id}/branches/${encodeURIComponent(branch.branch_name)}`
                                  ? "app-tree-link app-tree-link-branch app-tree-link-active"
                                  : "app-tree-link app-tree-link-branch"
                              }
                            >
                              {branch.branch_name}
                            </Link>
                          ))}
                        </div>
                      ) : null}
                    </div>
                  );
                })}
              </nav>
            </aside>
            <main className="app-main">
              <Outlet />
            </main>
          </div>
        </Layout.Content>
      </Layout>
    </AppProviders>
  );
}

export const rootRoute = createRootRoute({
  component: RootLayout,
});
