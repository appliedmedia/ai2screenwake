# mcp_protocol.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: ScreenWake local MCP protocol
  Scenario: Discovery exposes exactly the three approved owner controls
    Given a local MCP stdio session
    When the client initializes and lists tools
    Then exactly is_on turn_on and turn_off are advertised with empty inputs

  Scenario Outline: Invalid control arguments are protocol errors
    Given an initialized local MCP session
    When the client calls turn_on with arguments <arguments>
    Then JSON-RPC error -32602 is returned
    Examples:
      | arguments          |
      | {"enabled": true}  |
      | []                 |
      | null               |
      | "on"               |

  Scenario: Unknown tools never become successful tool results
    Given an initialized local MCP session
    When the client calls unknown tool idle_info
    Then JSON-RPC error -32601 is returned

  Scenario: Unknown methods receive a method-not-found response
    Given an initialized local MCP session
    When the client calls an unknown method
    Then JSON-RPC error -32601 is returned

  Scenario: Malformed JSON receives a parse error and the stream recovers
    Given an initialized local MCP session
    When malformed JSON is followed by a ping
    Then a parse error precedes the successful ping

  Scenario: Invalid JSON-RPC envelopes do not dispatch controls
    Given an initialized local MCP session
    When a request uses the wrong JSON-RPC version
    Then JSON-RPC error -32600 is returned

  Scenario: Unsupported initialization revisions are explicit
    Given a local MCP stdio session
    When the client initializes with an unsupported revision
    Then JSON-RPC error -32022 is returned

  Scenario: Oversized lines are bounded and the stream recovers
    Given an initialized local MCP session
    When an oversized line is followed by a ping
    Then an invalid-request error precedes the successful ping

  Scenario: The actual unbundled executable is discoverable but never invents an owner
    Given the built MCP subprocess
    When the client initializes and lists tools
    Then exactly is_on turn_on and turn_off are advertised with empty inputs
    When the client calls is_on
    Then the unavailable result has null status and an explicit public error

  Scenario: Closed stdout cannot leave the executable waiting on open stdin
    Given the built MCP subprocess
    When the client initializes and lists tools
    When stdout closes while stdin remains open
    Then the executable exits with an explicit transport failure
# end mcp_protocol.feature
