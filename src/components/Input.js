import { Component } from "react"
import { Input, InputGroup, InputLeftAddon, InputRightElement } from "@chakra-ui/react";

class IInput extends Component {
  constructor(props) {
    super(props);
    this.state = { innerValue: this.props.value }
  }

  onChange = e => this.setState({ innerValue: e.target.value });

  UNSAFE_componentWillReceiveProps(nextProps) {
    const { value } = nextProps;
    if (this.state.innerValue !== value) this.setState({ innerValue: value });
  }

  render() {
    return (
      <InputGroup>
        <InputLeftAddon
          children={this.props.tag}
          backgroundColor="#282828"
          color="#d79921"
          border="solid"
          borderWidth="2px"
          borderColor="#d79921"
          fontWeight="bold"
          textAlign="center"
          whiteSpace="nowrap"
        />
        <Input
          fontSize="xl"
          color="#fbf1c7"
          borderWidth="2px"
          borderColor="#d79921"
          value={this.state.innerValue}
          onChange={this.onChange}
        />
        <InputRightElement>
          {this.props.element}
        </InputRightElement>
      </InputGroup>
    )
  }
}

export default IInput