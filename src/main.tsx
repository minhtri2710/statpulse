import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { IslandMirrorPill } from "./features/island/IslandMirrorPill";
import "./index.css";

const isIslandMirror =
  new URLSearchParams(window.location.search).get("role") === "island-mirror";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isIslandMirror ? <IslandMirrorPill /> : <App />}
  </React.StrictMode>
);
