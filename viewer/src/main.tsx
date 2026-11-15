import React from "react";
import ReactDOM from "react-dom/client";
import { createHashRouter, RouterProvider } from "react-router-dom";
import App from "./App.tsx";
import Overview from "./pages/Overview.tsx";
import Explorer from "./pages/Explorer.tsx";
import TraceDetail from "./pages/TraceDetail.tsx";
import Settings from "./pages/Settings.tsx";
import "./styles.css";

// Hash routing keeps deep links working when served as static files from the
// collector without server-side rewrites.
const router = createHashRouter([
  {
    path: "/",
    element: <App />,
    children: [
      { index: true, element: <Overview /> },
      { path: "traces", element: <Explorer /> },
      { path: "traces/:traceId", element: <TraceDetail /> },
      { path: "settings", element: <Settings /> },
    ],
  },
]);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);
