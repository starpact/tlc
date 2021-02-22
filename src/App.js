import { useState, useEffect } from "react"
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

  return (
    <ChakraProvider>
      <Box h="800px" bg="#282828">
        <IAlert errMsg={errMsg} onClose={() => setErrMsg("")} />
        <SimpleGrid columns={2}>
          <Button rounded={false} bg="#98971a" color="#32302f" onClick={() => setAppState(0)}>路径与同步</Button>
          <Button rounded={false} bg="#458588" color="#32302f" onClick={() => setAppState(1)}>求解设置</Button>
        </SimpleGrid>
        <Center>
          <Heading color="#689d6a">当前实验组：{config.case_name}</Heading>
        </Center>
        <Stack key={new Date().getTime()} >
          {appState === 0 &&
            <BasicSettings
              config={config}
              setConfig={setConfig}
              setErrMsg={setErrMsg}
            />}
          {appState === 1 &&
            <SolveSettings
              config={config}
              setConfig={setConfig}
              setErrMsg={setErrMsg}
            />}
        </Stack>
      </Box>
    </ChakraProvider >
  )
}

export default App;
