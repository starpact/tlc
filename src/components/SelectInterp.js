import { HStack, Select, Stack } from "@chakra-ui/react"
import { useState } from "react"
import IButton from "./Button";
import IInput from "./Input";

function SelectInterp({ value, onSubmit }) {
  const [type, setType] = useState(() => {
    if (!!value.Horizontal) return "Horizontal";
    if (!!value.HorizontalExtra) return "HorizontalExtra";
    if (!!value.Vertical) return "Vertical";
    if (!!value.VerticalExtra) return "VerticalExtra";
    if (!!value.Bilinear) return "Bilinear";
    if (!!value.BilinearExtra) return "BilinearExtra";
    return "";
  });

  const [shape, setShape] = useState(() => {
    if (!!value.Horizontal) return value.Horizontal;
    if (!!value.HorizontalExtra) return value.HorizontalExtra
    if (!!value.Vertical) return value.Vertical;
    if (!!value.VerticalExtra) return value.VerticalExtra;
    if (!!value.Bilinear) return value.Bilinear;
    if (!!value.BilinearExtra) return value.BilinearExtra;
    return ["", ""];
  });

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
              const arr = shape;
              arr[0] = parseInt(v);
              setShape(shape);
            }}
            mutable
          />
          <IInput
            leftTag="热电偶列数"
            value={shape[1]}
            onBlur={v => {
              const arr = shape;
              arr[1] = parseInt(v);
              setShape(shape);
            }}
            mutable
          />
        </Stack>}
      <IButton text="提交" onClick={() => {
        if (type === "Bilinear") onSubmit({ Bilinear: shape })
        else onSubmit({ BilinearExtra: shape })
      }} />
    </HStack >
  )
}

export default SelectInterp