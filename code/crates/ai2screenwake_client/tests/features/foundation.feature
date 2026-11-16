# foundation.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Native-confirmed Actions foundation
  Scenario: A new owner does not advertise On-only measurements
    Given a newly created off projection
    When the projection crosses the JSON boundary
    Then protection is off without On-only fields

  Scenario: Requested protection stays pending before native confirmation
    Given an owner with an injected native boundary
    When protection is requested
    Then one native apply is pending without claiming on

  Scenario: Only both confirmed resources establish On
    Given an owner with an injected native boundary
    When protection is requested
    And native confirms 2 resources
    Then protection is on with empty unmeasured On-only fields

  Scenario: Partial acquisition dispatches an error before its failed projection
    Given an owner with an injected native boundary
    When protection is requested
    And native confirms 1 resources
    Then an attributed error precedes the unknown failed projection

  Scenario: Stale native confirmation cannot establish On
    Given an owner with an injected native boundary
    When protection is requested
    And a mismatched native confirmation arrives
    Then protection remains pending and a stale observation error is logged

  Scenario: Observer reentrancy preserves complete fan-out ordering
    Given an owner with an injected native boundary
    And two observers with a reentrant query observer
    When protection is requested
    Then both observers see the same ordered entries exactly once

  Scenario: Repeated setters share the pending native operation
    Given an owner with an injected native boundary
    When protection is requested twice
    Then one native apply is pending without claiming on

  Scenario: An opposite intent waits for native accounting before reconciliation
    Given an owner with an injected native boundary
    When protection is requested
    And protection is turned off before native completion
    And native confirms 2 resources
    Then the next native apply releases resources without claiming off

  Scenario: Protected-idle age is bounded by the current confirmed-On interval
    Given an owner with an injected native boundary
    When protection is requested
    And native confirms 2 resources
    And a query observes 12000 milliseconds of OS idle at monotonic time 6500
    Then protected-idle duration is 5 seconds

  Scenario: Idle sampling failure does not fabricate zero or false Off
    Given an owner with an injected native boundary
    When protection is requested
    And native confirms 2 resources
    And native idle sampling fails for a query
    Then protection stays on with null duration and an attributed sampling error

  Scenario: Show Log observes the same canonical ordered entries
    Given an owner with an injected native boundary
    When protection is requested
    And the native UI requests the log view
    Then the log view enumerates every prior dispatch exactly once

  Scenario: Missing native collaborators fail before startup
    When an owner is started without native collaborators
    Then startup explicitly fails

  Scenario: Missing required typed handlers fail at registration time
    When a registry is built without its required handler
    Then startup explicitly fails

  Scenario: Invalid native event minutes cannot enter the wire contract
    When invalid event minutes cross the JSON boundary
    Then each nonfinite or negative value is rejected

  Scenario: Sibling emissions share one correlation instance allocator
    Given a registry with sibling and nested snapshot emissions
    When the root query is emitted
    Then both snapshot continuations have distinct correlation instances

  Scenario: Synchronous native completion is observed after pending state
    Given an owner with a synchronous native completion
    When protection is requested
    Then pending is observed before native-confirmed On

  Scenario: Raw UI ingress cannot forge native resource confirmation
    Given an owner with an injected native boundary
    When protection is requested
    And native confirms 2 resources
    And the native result is replayed through raw UI ingress
    Then the ingress rejects it without another resource operation

  Scenario: Invalid control parameters retain the failed action identity
    Given an owner with an injected native boundary
    When a query with unexpected parameters enters raw UI ingress
    Then an attributed invalid-payload error is observed without native work
  @late_review
  Scenario: Exhausted inbound continuation metadata cannot stop subsequent dispatch
    Given an owner with an injected native boundary
    When an exhausted log-query continuation enters the owner
    Then a validation error and a later normal log response are observed

  @late_review
  Scenario: Continuation exhaustion cannot strand the journal dispatch worker
    Given a journal-dispatched Actions registry
    When an exhausted log-query continuation enters the owner
    Then a validation error and a later normal log response are observed
# end foundation.feature
