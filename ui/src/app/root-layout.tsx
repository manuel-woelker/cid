import { Breadcrumb, Layout, Space, Typography } from "antd";
import {
  Link,
  Outlet,
  createRootRoute,
  useRouterState,
} from "@tanstack/react-router";

import faviconUrl from "../assets/cid-favicon.png";
import { AppProviders } from "./providers";

const navItems = [{ to: "/", label: "Dashboard" }] as const;

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
          <Outlet />
        </Layout.Content>
      </Layout>
    </AppProviders>
  );
}

export const rootRoute = createRootRoute({
  component: RootLayout,
});
