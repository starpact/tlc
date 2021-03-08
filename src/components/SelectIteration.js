import { Box, HStack, Select, Stack } from "@chakra-ui/react";
import { useState } from "react";
import IButton from "./Button";
import IInput from "./Input";

function SelectIteration({ value, onSubmit, setErrMsg }) {
  const [type, setType] = useState(Object.keys(value)[0]);
  const [h0, setH0] = useState(Object.values(Object.values(value)[0])[0]);
  const [maxIterNum, setMaxIterNum] = useState(Object.values(Object.values(value)[0])[1]);

  return (
    <HStack>
      <Select
        w="200px"
        value={type}
        bg="#689d6a"
        color="#32302f"
        border="unset"
        fontWeight="bold"
        onChange={e => setType(e.target.value)}
        marginRight="9px"
      >
        <option value="NewtonTangent">牛顿切线</option>
        <option value="NewtonDown">牛顿下山</option>
      </Select>
      <Box w="350px" marginRight="9px">
        <IInput
          leftTag="对流换热系数初值"
          value={!!h0 && h0.toFixed(1)}
          onBlur={v => {
            const vv = parseFloat(v);
            if (isNaN(vv)) {
              setErrMsg(`不合法的迭代初值：${v}`);
              return;
            }
            setH0(vv);
          }}
          mutable
          rightTag="W/(m2·K)"
        />
      </Box>
      <Box w="200px" marginRight="9px">
        <IInput
          leftTag="最大迭代步数"
          value={maxIterNum}
          onBlur={v => {
            const vv = parseInt(v);
            if (isNaN(vv) || vv <= 0) {
              setErrMsg(`不合法的最大迭代步数：${v}`);
              return;
            }
            setMaxIterNum(vv);
          }}
          mutable
        />
      </Box>
      <IButton text="提交" onClick={() => {
        if (type === "NewtonTangent") onSubmit({ NewtonTangent: { h0, max_iter_num: maxIterNum } })
        else onSubmit({ NewtonDown: { h0, max_iter_num: maxIterNum } })
      }} />
    </HStack>
  )
}

export default SelectIteration