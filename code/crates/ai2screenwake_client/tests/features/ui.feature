# ui.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Generated native menu follows observed owner state
  Scenario: Confirmed owner On is projected through the one rendering observation stream
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    Then the generated observer receives the awake TV and observed On label

  Scenario: Requested On is never an optimistic awake icon
    Given a typed native menu observation is registered
    When the owner observes a pending enable intent
    Then the observed menu remains pending with a distinct non-color badge

  Scenario: Unconfirmed native errors never appear Off
    Given a typed native menu observation is registered
    When the owner observes unresolved protection
    Then the menu labels Error rather than falsely claiming Off

  Scenario: Sampling errors preserve independent confirmed protection truth
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And a native activity fault with private details is dispatched
    Then the menu keeps On with short error copy and no private chain

  @telemetry_ui
  Scenario Outline: Telemetry <family> errors preserve observed <phase> and available controls
    Given an observed <phase> native menu
    When the canonical telemetry <family> fault is dispatched
    Then observed <phase> is retained with a safe accessible telemetry error and unchanged controls

    Examples:
      | family        | phase |
      | Configuration | On    |
      | Configuration | Off   |
      | Transport     | On    |
      | Transport     | Off   |
      | Buffer        | On    |
      | Buffer        | Off   |

  @telemetry_ui
  Scenario Outline: Telemetry <family> errors cannot reopen controls during Quit
    Given an observed On native menu
    When native Quit is admitted without a terminal result
    And the canonical telemetry <family> fault is dispatched
    Then telemetry error visibility preserves in-progress Quit and protection truth

    Examples:
      | family        |
      | Configuration |
      | Transport     |
      | Buffer        |

  Scenario: Show Log projects canonical metadata rather than private payloads
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And a native activity fault with private details is dispatched
    And the existing log query is emitted
    Then the auxiliary log is visible with redacted ordered metadata

  Scenario: Dismissing the auxiliary log is idempotent and preserves protection
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And the existing log query is emitted
    And the log is dismissed twice
    Then it is closed once without changing the observed On state

  Scenario: Dismissal wins over a late canonical log reply
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And the existing log query is emitted
    And the log is dismissed twice
    And the old log query emits a late snapshot
    Then the dismissed log stays closed without another activation

  Scenario: An existing log does not steal focus on background owner observations
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And the existing log query is emitted
    And the owner observes confirmed protection On
    Then the log stays visible without a new activation instruction

  @log_health
  Scenario: Omitted canonical history is disclosed rather than presented as complete
    Given a typed native menu observation is registered
    When the canonical log snapshot reports earlier omitted history
    Then the log visibly discloses earlier missing history

  @log_health
  Scenario: Unhealthy canonical history is disclosed without exposing private faults
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And the canonical log snapshot reports a storage fault
    Then the log warns about unsaved history while preserving On and privacy

  Scenario: Dropping the generated observation token releases callbacks
    Given a typed native menu observation is registered
    When the generated observation token is dropped
    And the owner observes confirmed protection On
    Then the foreign observer is no longer retained or invoked

  Scenario: Off observations alone cannot authorize application termination
    Given a typed native menu observation is registered
    When native Quit is admitted without a terminal result
    And the owner observes confirmed Off
    Then Quit remains in progress without a terminal instruction

  Scenario: Failed Quit offers cleanup retry but never allows a new enable
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And native Quit is admitted without a terminal result
    And that Quit settles with a retained-resource error
    Then Quit is retryable without termination or re-enabled primary control

  @real_host
  Scenario: The actual generated host shares native control with its private IPC owner
    Given an actual generated native host is observed
    When its typed primary toggle is emitted
    Then confirmed On follows pending and the same IPC owner reports On
    When its typed Quit is emitted
    Then confirmed Off precedes terminal rendering and the private endpoint is removed

  @real_host
  Scenario: A second generated host fails explicitly without claiming Off
    Given an actual generated native host is observed
    When a second generated host attempts admission
    Then its error observation never claims Off and its Quit leaves the first owner available
  @ordered_projection
  Scenario: Delayed old snapshots and changes cannot replace a newer Off menu
    Given a typed native menu observation is registered
    When newer Off is followed by older On snapshots and changes
    Then the menu never renders the older awake state

  @ordered_projection
  Scenario: Equal-revision observations are not mistaken for older state
    Given a typed native menu observation is registered
    When a same-revision snapshot supplies confirmed On after Pending
    Then the same-revision observation is rendered rather than discarded

  @ordered_projection
  Scenario: Persistent On completion followed by Off never renders an older revision
    Given a persistent owner queue with a native completion journal barrier
    When Off is queued behind the journalled On completion before derived state drains
    Then rendered protection revisions never regress and menu and remote status agree Off
  @late_review
  Scenario Outline: Foreign shutdown preserves <gate> evidence across explicit retries
    When the private disposal gate exercises <gate>
    Then terminal delivery requires the retained completion and genuine cleanup evidence

    Examples:
      | gate       |
      | completion |
      | late_error |
      | peer_error |
# end ui.feature
