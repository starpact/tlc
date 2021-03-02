import { HStack, Select, Stack } from "@chakra-ui/react";
import { useState } from "react";
import IButton from "./Button";
import IInput from "./Input";

function SelectInterp({ value, onSubmit, setErrMsg }) {
  const [type, setType] = useState(Object.keys(value)[0]);

  const [shape, setShape] = useState(Object.values(value)[0]);

  return (
    <HStack>
      <Select
        w="160px"
        textAlign="center"
        value={type}
        bg="#689d6a"
        color="#32302f"
        border="unset"
        fontWeight="bold"
        onChange={e => setType(e.target.value)}
      >
        <option value="Horizontal">水平</option>
        <option value="HorizontalExtra">水平（外插）</option>
        <option value="Vertical">垂直</option>
        <option value="VerticalExtra">垂直（外插）</option>
        <option value="Bilinear">双线性</option>
        <option value="BilinearExtra">双线性（外插）</option>
      </Select>
      {(type === "Bilinear" || type === "BilinearExtra") &&
        <Stack>
          <IInput
            leftTag="热电偶行数"
            value={shape[0]}
            onBlur={v => {
              const vv = parseInt(v);
              if (!vv || vv <= 0) {
                setErrMsg(`不合法的热电偶行数：${v}`);
                return;
              }
              setShape([vv, shape[1]]);
            }}
            mutable
          />
          <IInput
            leftTag="热电偶列数"
            value={shape[1]}
            onBlur={v => {
              const vv = parseInt(v);
              if (!vv || vv <= 0) {
                setErrMsg(`不合法的热电偶列数：${v}`);
                return;
              }
              setShape([shape[0], vv]);
            }}
            mutable
          />
        </Stack>}
      <IButton text="提交" onClick={() => {
        let interpMethod = new Object();
        interpMethod[type] = (type === "Bilinear" || type === "BilinearExtra") ? shape : null;
        onSubmit(interpMethod);
      }} />
    </HStack >
  )
}

export default SelectInterp