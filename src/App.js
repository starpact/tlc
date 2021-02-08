import React from "react";
import {
  ChakraProvider,
  Box,
  HStack,
  Stack,
} from "@chakra-ui/react";
import { FaFileVideo, FaFileCsv, FaSave } from "react-icons/fa"
import * as tauri from 'tauri/api/tauri'
import * as dialog from 'tauri/api/dialog'

import IButton from "./components/Button"
import IIConButton from "./components/IconButton"
import IInput from "./components/Input"
import IAlert from "./components/Alert";

function App() {
  const [config, setConfig] = React.useState("");
  const [errorMsg, setErrorMsg] = React.useState("");

  function getConfig() {
    tauri.promisified({
      cmd: "getConfig",
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrorMsg(err));
  }

  function loadConfig() {
    dialog.open().then(path => {
      tauri.promisified({
        cmd: "loadConfig",
        config_path: path,
      })
        .then(ok => setConfig(ok))
        .catch(err => setErrorMsg(err));
    });
  }

  function setVideoPath() {
    dialog.open().then(path => {
      tauri.promisified({
        cmd: "setVideoPath",
        video_path: path,
      })
        .then(ok => setConfig(ok))
        .catch(err => setErrorMsg(err));
    });
  }

  function setDAQPath() {
    dialog.open().then(path => {
      tauri.promisified({
        cmd: "setDAQPath",
        daq_path: path,
      })
        .then(ok => setConfig(ok))
        .catch(err => setErrorMsg(err));
    });
  }

  return (
    <ChakraProvider>
      <Box h="800px" bg="#282828">
        <IAlert errorMsg={errorMsg} onClose={() => setErrorMsg("")} />
        <IButton text="加载默认配置" onClick={getConfig} />
        <IButton text="导入配置" onClick={loadConfig} />
        <Stack>
          <HStack>
            <IInput
              tag="视频文件路径"
              value={config.video_path}
              element={<IIConButton icon={<FaFileVideo />} onClick={setVideoPath} />}
            />
          </HStack>
          <HStack>
            <IInput
              tag="数采文件路径"
              value={config.daq_path}
              element={<IIConButton icon={<FaFileCsv />} onClick={setDAQPath} />}
            />
          </HStack>
          <HStack>
            <IInput
              tag="起始帧数"
              value={config.start_frame}
              element={<IIConButton icon={<FaSave />} onClick={setVideoPath} />}
            />
          </HStack>
          <HStack>
            <IInput
              tag="起始行数"
              value={config.start_row}
              element={<IIConButton icon={<FaSave />} onClick={setDAQPath} />}
            />
          </HStack>
        </Stack>
      </Box>
    </ChakraProvider >
  );
}

export default App;
