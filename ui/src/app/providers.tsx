import type { PropsWithChildren } from "react";
import { App as AntdApp, ConfigProvider, theme } from "antd";

export function AppProviders({ children }: PropsWithChildren) {
  return (
    <ConfigProvider
      theme={{
        algorithm: theme.defaultAlgorithm,
        token: {
          colorPrimary: "#0f766e",
          colorInfo: "#0f766e",
          colorSuccess: "#166534",
          colorWarning: "#b45309",
          colorError: "#b91c1c",
          colorBgBase: "#f4efe6",
          colorTextBase: "#1f2937",
          borderRadius: 14,
          fontFamily: '"IBM Plex Sans", "Avenir Next", "Segoe UI", sans-serif',
        },
      }}
    >
      <AntdApp>{children}</AntdApp>
    </ConfigProvider>
  );
}
