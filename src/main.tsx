import React, { Suspense, lazy } from "react";
import ReactDOM from "react-dom/client";
import "./index.css";

// Each window role loads only its own entry code: mirror pills never parse the
// dashboard.
const App = lazy(() => import("./App"));
const IslandMirrorPill = lazy(() =>
  import("./features/island/IslandMirrorPill").then((m) => ({ default: m.IslandMirrorPill })),
);

const isIslandMirror =
  new URLSearchParams(window.location.search).get("role") === "island-mirror";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Suspense fallback={null}>{isIslandMirror ? <IslandMirrorPill /> : <App />}</Suspense>
  </React.StrictMode>
);
