import { useState } from "react"
import {
  ChakraProvider,
  Center,
  Heading,
  Box,
  Stack,
  Button,
  SimpleGrid,
} from "@chakra-ui/react"

import IAlert from "./components/Alert";

import SolveSettings from "./pages/SolveSettings"
import BasicSettings from "./pages/BasicSettings"

function App() {
  const [appState, setAppState] = useState(0);
  const [errMsg, setErrMsg] = useState("");
  const [config, setConfig] = useState("");

  function awsl(msg) {
    if (msg === "") setErrMsg("");
    else if (errMsg === "") setErrMsg(msg);
  }

  return (
    <ChakraProvider>
      <Box h="800px" bg="#282828">
        <IAlert errMsg={errMsg} onClose={() => setErrMsg("")} />
        {errMsg === "" &&
          <SimpleGrid columns={2}>
            <Button rounded={false} bg="#98971a" color="#32302f" onClick={() => setAppState(0)}>路径与同步</Button>
            <Button rounded={false} bg="#458588" color="#32302f" onClick={() => setAppState(1)}>求解设置</Button>
          </SimpleGrid>
        }
        <Center>
          <Heading
            color="#689d6a"
            marginBottom="5px"
            fontSize="3xl"
          >
            当前实验组：{config.case_name}
          </Heading>
        </Center>
        {/* <Stack key={`${JSON.stringify(config)}`}> */}
        <Stack key={`${JSON.stringify(config)}${errMsg}`}>
          {appState === 0 &&
            <BasicSettings
              config={config}
              setConfig={setConfig}
              awsl={awsl}
            />}
          {appState === 1 &&
            <SolveSettings
              config={config}
              setConfig={setConfig}
              awsl={awsl}
            />}
        </Stack>
      </Box>
    </ChakraProvider >
  )
}

export default App;
