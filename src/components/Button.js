import { Button, Tooltip } from "@chakra-ui/react";

function IButton({ text, onClick, hover, size }) {
  return (
    <Tooltip label={hover} backgroundColor="#3c3836" color="#fbf1c7">
      <Button
        size={size || "md"}
        boxShadow="dark-lg"
        backgroundColor="#458588"
        color="#fbf1c7"
        onClick={onClick}
        whiteSpace="nowrap"
      >
        {text}
      </Button>
    </Tooltip>
  )
}

export default IButton
