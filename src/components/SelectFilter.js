import { HStack, Select } from "@chakra-ui/react"
import { useState } from "react"
import IButton from "./Button";
import IInput from "./Input";

function SelectFilter({ value, onSubmit }) {
  const [innerValue, setInnerValue] = useState(value || "");

  const onSelectChange = v => {
    switch (v.target.value) {
      case "Median": setInnerValue({ Median: 20 }); break;
      case "Wavelet": setInnerValue({ Wavelet: 0.5 }); break;
      default: setInnerValue("No");
    }
  }

  return (
    <HStack w="400px">
      <Select
        w="200px"
        value={!!innerValue.Median ? "Median" : !!innerValue.Wavelet ? "Wavelet" : innerValue}
        bg="#689d6a"
        color="#282828"
        border="unset"
        fontWeight="bold"
        onChange={onSelectChange}
      >
        <option value="No">无</option>
        <option value="Median">中值</option>
        <option value="Wavelet">小波</option>
      </Select>
      {!!innerValue.Median &&
        <IInput
          leftTag="窗口宽度"
          value={innerValue.Median}
          onBlur={v => setInnerValue({ Median: parseInt(v) })}
          mutable
        />}
      {!!innerValue.Wavelet &&
        <IInput
          leftTag="滤波阈值"
          value={innerValue.Wavelet}
          onBlur={v => setInnerValue({ Wavelet: parseFloat(v) })}
          mutable
        />}
      <IButton text="提交" onClick={() => onSubmit(innerValue)} />
    </HStack>
  )
}

export default SelectFilter