import React, { useState } from "react";
import { Stack, Center } from "@chakra-ui/react";
import IInput from "./Input";
import IButton from "./Button";

function IPopover({ id, x, y, updateThermocouple }) {
  const [xx, setXx] = useState(x);
  const [yy, setYy] = useState(y);

  return (
    <Center>
      <Stack w="150px" marginRight="20px">
        <IInput leftTag="x: " value={xx} mutable onBlur={setXx} />
        <IInput leftTag="y: " value={yy} mutable onBlur={setYy} />
      </Stack>
      <IButton text="保存" onClick={() => updateThermocouple(id, xx, yy)} />
    </Center>
  )
}

export default IPopover