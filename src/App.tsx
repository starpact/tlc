import React from 'react';

import { invoke } from "@tauri-apps/api/tauri";

function App() {
  function load_config() {
    console.log("aaaaaaaa");
    invoke<string>("load_config")
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
