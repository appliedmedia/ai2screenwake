# runtime.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Shared-owner request and episode isolation
  Scenario: Concurrent queries retain both observations when native replies in reverse order
    Given a confirmed On shared owner
    When two independent queries are sampled in reverse order
    Then both queries publish an ordered observed snapshot

  Scenario: Every repeated setter settles only after its shared native completion
    Given a new shared owner
    When two setters share one native completion
    Then both distinct setter requests settle successfully

  Scenario: Every superseded setter receives an explicit terminal error
    Given a new shared owner
    When two pending On setters are superseded by Off
    Then both old setters receive superseded errors and Off is still pending

  Scenario: A timed-out setter is not completed again by a late native reply
    Given a new shared owner
    When a setter expires before its native reply
    Then that setter has exactly one attributed timeout result

  Scenario: Dropping a scoped observation stops future delivery
    Given a new shared owner
    When a scoped result observer is dropped before a query
    Then it receives no later query entries

  Scenario: The real deadline scheduler settles an unresponsive native operation
    Given an owner with a 20 millisecond operation deadline
    When protection is requested without a native reply
    Then the scheduler emits one timeout result within one second

  Scenario: Pending request notifications do not keep a disposed owner alive
    Given a new shared owner
    When its native callback and last owner port are dropped while pending
    Then the notification continuation cannot resurrect that owner

  Scenario: Quit settles only after explicit native Off cleanup
    Given a confirmed On shared owner
    When Quit is requested and native cleanup confirms zero resources
    Then Quit has one confirmed Off result
# end runtime.feature
