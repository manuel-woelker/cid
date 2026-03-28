import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";

import faviconUrl from "./assets/cid-favicon.png";
import { router } from "./app/router";

import "antd/dist/reset.css";
import "./styles/global.css";

let faviconLink = document.querySelector<HTMLLinkElement>("link[rel='icon']");

if (!faviconLink) {
  faviconLink = document.createElement("link");
  faviconLink.rel = "icon";
  document.head.appendChild(faviconLink);
}

faviconLink.href = faviconUrl;

const root = document.getElementById("root");

if (!root) {
  throw new Error("missing root element");
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);
