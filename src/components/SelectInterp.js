import { HStack, Select, Stack } from "@chakra-ui/react"
import { useState } from "react"
import IButton from "./Button";
import IInput from "./Input";

function SelectInterp({ value, onSubmit }) {
  const [innerValue, setInnerValue] = useState(value || "");

  const onSelectChange = v => {
    if (v.target.value === "Bilinear") setInnerValue({ Bilinear: ["", ""] })
    else if (v.target.value === "BilinearExtra") setInnerValue({ BilinearExtra: ["", ""] });
    else setInnerValue(v.target.value);
  }

  return (
    <HStack w="600px">
      <Select
        w="200px"
        value={!!innerValue.Bilinear ? "Bilinear" : !!innerValue.BilinearExtra ?
          "BilinearExtra" : innerValue}
        bg="#689d6a"
        color="#282828"
        border="unset"
        fontWeight="bold"
        onChange={onSelectChange}
      >
        <option value="Horizontal">水平</option>
        <option value="HorizontalExtra">水平（外插）</option>
        <option value="Vertical">垂直</option>
        <option value="VerticalExtra">垂直（外插）</option>
        <option value="Bilinear">双线性</option>
        <option value="BilinearExtra">双线性（外插）</option>
      </Select>
      {!!innerValue.Bilinear &&
        <Stack>
          <IInput
            leftTag="热电偶行数"
            value={innerValue.Bilinear[0]}
            onBlur={v => {
              const arr = innerValue.Bilinear.concat();
              arr[0] = parseInt(v);
              setInnerValue({ Bilinear: arr });
            }}
            mutable
          />
          <IInput
            leftTag="热电偶列数"
            value={innerValue.Bilinear[1]}
            onBlur={v => {
              const arr = innerValue.Bilinear.concat();
              arr[1] = parseInt(v);
              setInnerValue({ Bilinear: arr });
            }}
            mutable
          />
        </Stack>}
      {!!innerValue.BilinearExtra &&
        <Stack>
          <IInput
            leftTag="热电偶行数"
            value={innerValue.BilinearExtra[0]}
            onBlur={v => {
              const arr = innerValue.BilinearExtra.concat();
              arr[0] = parseInt(v);
              setInnerValue({ BilinearExtra: arr });
            }}
            mutable
          />
          <IInput
            leftTag="热电偶列数"
            value={innerValue.BilinearExtra[1]}
            onBlur={v => {
              const arr = innerValue.BilinearExtra.concat();
              arr[1] = parseInt(v);
              setInnerValue({ BilinearExtra: arr });
            }}
            mutable
          />
        </Stack>}
      <IButton text="提交" onClick={() => onSubmit(innerValue)} />
    </HStack>
  )
}

export default SelectInterp