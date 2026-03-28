import { Layout, Space, Typography } from "antd";
import { Link, Outlet, createRootRoute } from "@tanstack/react-router";

import faviconUrl from "../assets/cid-favicon.png";
import { AppProviders } from "./providers";

const navItems = [{ to: "/", label: "Dashboard" }] as const;

function RootLayout() {
  return (
    <AppProviders>
      <Layout className="app-layout">
        <Layout.Header className="app-header">
          <div className="app-brand">
            <img src={faviconUrl} alt="cid" className="app-logo" />
            <div className="app-brand-copy">
              <Typography.Title level={3}>cid</Typography.Title>
              <Typography.Text type="secondary">
                Local-first CI with a dashboard that does not suck
              </Typography.Text>
            </div>
          </div>
          <Space size="middle">
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
