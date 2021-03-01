import { HStack, Select, Stack } from "@chakra-ui/react";
import { useState } from "react";
import IButton from "./Button";
import IInput from "./Input";

function SelectIteration({ value, onSubmit }) {
  const [type, setType] = useState(() => {
    if (!value) return "";
    if (!!value.NewtonTangent) return "NewtonTangent";
    return "NewtonDown";
  });

  const [h0, setH0] = useState(() => {
    if (!value) return 50;
    if (!!value.NewtonTangent) return value.NewtonTangent.h0;
    return value.NewtonDown.h0;
  });

  const [maxIterNum, setMaxIterNum] = useState(() => {
    if (!value) return 10;
    if (!!value.NewtonTangent) return value.NewtonTangent.max_iter_num;
    return value.NewtonDown.max_iter_num;
  })

  return (
    <HStack w="600px">
      <Select
        w="200px"
        value={type}
        bg="#689d6a"
        color="#32302f"
        border="unset"
        fontWeight="bold"
        onChange={e => setType(e.target.value)}
      >
        <option value="NewtonTangent">牛顿切线</option>
        <option value="NewtonDown">牛顿下山</option>
      </Select>
      <Stack>
        <IInput
          leftTag="初值"
          value={!!h0 && h0.toFixed(1)}
          onBlur={v => setH0(parseFloat(v))}
          mutable
        />
        <IInput
          leftTag="最大迭代步数"
          value={maxIterNum}
          onBlur={v => setMaxIterNum(parseInt(v))}
          mutable
        />
      </Stack>
      <IButton text="提交" onClick={() => {
        if (type === "NewtonTangent") onSubmit({ NewtonTangent: { h0, max_iter_num: maxIterNum } })
        else onSubmit({ NewtonDown: { h0, max_iter_num: maxIterNum } })
      }} />
    </HStack>
  )
}

export default SelectIteration