import { Text, Tooltip } from "@chakra-ui/react";

function ITag({ text, w, h, hover, onMouseDown }) {
  return (
    <Tooltip label={hover} backgroundColor="#3c3836" color="#fbf1c7">
      <Text
        textAlign="center"
        rounded="md"
        w={w}
        h={h}
        bgColor="#98971a"
        onMouseDown={onMouseDown}
        color="#32302f"
        fontWeight="bold"
        fontSize="sm">
        {text}
      </Text>
    </Tooltip>
  )
}

export default ITag