# mcp_owner.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: MCP controls use the real shared owner IPC
  Scenario: Initialization and discovery never acquire protection
    Given two MCP sessions connected to one real private owner
    When the first client lists tools without issuing a control
    Then the native collaborator has received no apply

  Scenario: Two clients and native control share one observed state
    Given two MCP sessions connected to one real private owner
    When the clients interleave on query and native off
    Then each result matches its request and the shared native owner

  Scenario: Repeated setters settle separately after the same convergence
    Given two MCP sessions connected to one real private owner
    When one client sends two on requests before native convergence
    Then both request IDs succeed with just one native apply

  Scenario: Opposite client intents preserve superseded request attribution
    Given two MCP sessions connected to one real private owner
    When another client requests off before on converges
    Then the on request is superseded and only the off request succeeds

  Scenario: Native acquisition faults remain explicit tool failures
    Given two MCP sessions connected to one real private owner
    When native acquisition fails during a turn_on call
    Then the tool error preserves unknown protection without exposing failed_genus

  Scenario: Sampling faults preserve independently observed On
    Given two MCP sessions connected to one real private owner
    When an On query encounters a native idle sampling failure
    Then On remains true with null duration and an explicit sampling error

  Scenario: A real unavailable owner does not become false Off
    Given a real private owner location without a listener
    When the client calls is_on
    Then the unavailable result has null status and an explicit public error

  Scenario: Owner deadlines are forwarded without fabricated success
    Given a real private owner with a short test deadline
    When native convergence misses the owner deadline
    Then the tool result reports the owner's timeout

  Scenario: MCP disconnection relinquishes notification but not admitted intent
    Given two MCP sessions connected to one real private owner
    When an on caller disconnects before native convergence
    Then the remaining client observes On after the admitted apply finishes

  Scenario: Cancellation releases transport interest without reversing the owner
    Given two MCP sessions connected to one real private owner
    When an on caller cancels its request before native convergence
    Then a fresh query succeeds after the admitted apply finishes

  Scenario: Duplicate live IDs cannot steal the original result
    Given two MCP sessions connected to one real private owner
    When a client duplicates an outstanding on request ID
    Then the duplicate is rejected and the original request still settles

  Scenario: Excess in-flight requests fail within a bounded ledger
    Given two MCP sessions connected to one real private owner
    When a client submits more than thirty-two outstanding controls
    Then the extra request is rejected and all admitted requests settle
# end mcp_owner.feature
