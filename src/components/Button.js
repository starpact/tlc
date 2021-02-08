import { Button } from "@chakra-ui/react";

function IButton({ text, onClick }) {
  return (
    <Button
      size="md"
      boxShadow="dark-lg"
      backgroundColor="#458588"
      color="#fbf1c7"
      onClick={onClick}
      whiteSpace="nowrap"
    >
      {text}
    </Button>
  )
}

export default IButton