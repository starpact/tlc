import React from 'react';

import { invoke } from "@tauri-apps/api/tauri";

// TODO: Migrate FE code from old version.
function App() {
  function get_name() {
    invoke<string>("get_name")
      .then(msg => console.log(msg))
      .catch(console.error);
  }
  function set_name(name: string) {
    invoke<string>("set_name", { name }).catch(console.error);
  }

  return (
    <div className="App">
      <header className="App-header">
      </header>
      <br />
      <button onClick={get_name}>get_name</button>
      <input type="text" name="set_name" onInput={e => set_name((e.target as HTMLInputElement).value)} />
      <br />
    </div >
  );
}

export default App;
