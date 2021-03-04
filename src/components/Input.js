import { useState } from "react";
import {
  Input,
  InputGroup,
  InputLeftAddon,
  Tooltip,
  Text,
  InputRightAddon,
  InputRightElement,
} from "@chakra-ui/react";

function IInput({
  leftTag,
  hover,
  value,
  onBlur,
  mutable,
  placeholder,
  rightTag,
  element
}) {
  const [innerValue, setInnerValue] = useState(value);

  return (
    <InputGroup>
      {!!leftTag && <InputLeftAddon
        backgroundColor="#282828"
        color="#d79921"
        border="solid"
        borderWidth="2px"
        borderColor="#d79921"
        fontWeight="bold"
        textAlign="center"
        whiteSpace="nowrap"
      >
        <Tooltip label={hover} backgroundColor="#3c3836" color="#fbf1c7">
          <Text>{leftTag}</Text>
        </Tooltip>
      </InputLeftAddon>}
      <Input
        fontSize="xl"
        color={!!mutable ? "#fbf1c7" : "#dfc4a1"}
        borderWidth="2px"
        borderColor="#d79921"
        value={innerValue}
        onChange={e => setInnerValue(e.target.value)}
        onBlur={e => { !!mutable && !!onBlur && onBlur(e.target.value) }}
        placeholder={placeholder}
        readOnly={!!mutable ? false : true}
      />
      {!!element && <InputRightElement>{element}</InputRightElement>}
      {!!rightTag &&
        <InputRightAddon
          children={rightTag}
          backgroundColor="#282828"
          color="#cc241d"
          border="solid"
          borderWidth="2px"
          borderColor="#d79921"
          fontWeight="bold"
          textAlign="center"
          whiteSpace="nowrap"
        />
      }
    </InputGroup>
  )
}

export default IInput
