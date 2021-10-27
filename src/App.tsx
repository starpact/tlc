import { invoke } from "@tauri-apps/api/tauri";

function App() {
  invoke("my_custom_command")
    .then(msg => console.log(msg))
    .catch(err => console.error(err));

  return (
    <div>
      Hello TLC
    </div>
  )
}

export default App
