import { HStack, Select, Box } from "@chakra-ui/react";
import { useEffect, useState } from "react";
import IButton from "./Button";
import IInput from "./Input";

function SelectInterp({ value, onSubmit, setErrMsg }) {
  const [type, setType] = useState(() => {
    if (!!value.Bilinear) return "Bilinear";
    if (!!value.BilinearExtra) return "BilinearExtra";
    return value;
  });
  const [shape, setShape] = useState(() => {
    if (!!value.Bilinear) return value.Bilinear;
    if (!!value.BilinearExtra) return value.BilinearExtra;
    return ["", ""];
  });

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
        marginRight="9px"
      >
        <option value="Horizontal">水平</option>
        <option value="HorizontalExtra">水平（外插）</option>
        <option value="Vertical">垂直</option>
        <option value="VerticalExtra">垂直（外插）</option>
        <option value="Bilinear">双线性</option>
        <option value="BilinearExtra">双线性（外插）</option>
      </Select>
      {(type === "Bilinear" || type === "BilinearExtra") &&
        <HStack marginRight="9px">
          <Box w="167px" marginRight="9px">
            <IInput
              leftTag="热电偶行数"
              value={shape[0]}
              onBlur={v => {
                const vv = parseInt(v);
                if (isNaN(vv) || vv < 2) {
                  setErrMsg(`不合法的热电偶行数：${v}`);
                  return;
                }
                setShape([vv, shape[1]]);
              }}
              mutable
            />
          </Box>
          <Box w="167px">
            <IInput
              leftTag="热电偶列数"
              value={shape[1]}
              onBlur={v => {
                const vv = parseInt(v);
                if (isNaN(vv) || vv < 2) {
                  setErrMsg(`不合法的热电偶列数：${v}`);
                  return;
                }
                setShape([shape[0], vv]);
              }}
              mutable
            />
          </Box>
        </HStack>}
      <IButton text="插值" onClick={() => {
        let interpMethod = {};
        interpMethod[type] = (type === "Bilinear" || type === "BilinearExtra") ? shape : null;
        onSubmit(interpMethod);
      }} />
    </HStack>
  )
}

export default SelectInterp