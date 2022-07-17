import React from 'react';

import { invoke } from "@tauri-apps/api/tauri";

// TODO: Migrate FE code from old version.
function App() {
  function load_config() {
    invoke<string>("load_config", { configPath: "../src-tauri/config/default.toml" })
      .then((msg) => console.log(msg))
      .catch(console.error);
  }

  return (
    <div className="App">
      <header className="App-header">
      </header>
      <br />
      <button onClick={load_config}>load_config</button>
      <br />
    </div >
  );
}

export default App;
