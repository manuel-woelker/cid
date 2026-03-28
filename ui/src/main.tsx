import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";

import { router } from "./app/router";

import "antd/dist/reset.css";
import "./styles/global.css";

const root = document.getElementById("root");

if (!root) {
  throw new Error("missing root element");
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);
