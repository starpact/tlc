import React from "react";
import {
  ChakraProvider,
  Button,
} from "@chakra-ui/react";

function App() {
  return (
    <ChakraProvider>
      <Button colorScheme="teal" size="lg">AAA</Button>
    </ChakraProvider>
  );
}

export default App;
