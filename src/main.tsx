import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { markStartupTrace } from "./startupTrace";

markStartupTrace("main_module_enter");

const rootElement = document.getElementById("root") as HTMLElement;
markStartupTrace("react_create_root_begin");
const root = ReactDOM.createRoot(rootElement);
markStartupTrace("react_create_root_end");

root.render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
markStartupTrace("react_render_scheduled");
