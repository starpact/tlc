
import React from "react";
import {
  Alert,
  AlertIcon,
  AlertDescription,
  CloseButton,
} from "@chakra-ui/react";

function IAlert({ errorMsg, onClose }) {
  return (
    <Alert
      display={errorMsg === "" ? "none" : "flex"}
      status="error"
      bg="#f38019"
    >
      <AlertIcon color="#cc241d" />
      <AlertDescription color="#1d2021">{errorMsg}</AlertDescription>
      <CloseButton position="absolute" right="8px" top="8px" onClick={onClose} />
    </Alert>
  )
}

export default IAlert