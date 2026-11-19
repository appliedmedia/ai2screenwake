# lifecycle.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
@lifecycle
Feature: Native sleep replies preserve current ownership and forced-sleep allowance
  @idle_veto
  Scenario: Admitted native No Sleep cancels an actual idle request and keeps both assertions
    Given admitted No Sleep with controlled OS sleep replies
    When the controlled OS asks to idle sleep with token 712
    Then only that token is cancelled and its exact acquisition receipt preserves both assertions

  @idle_veto
  Scenario Outline: An <state> native owner never vetoes idle sleep
    Given admitted No Sleep with controlled OS sleep replies
    When native veto admission becomes <state>
    And the controlled OS asks to idle sleep with token 713
    Then that idle request is allowed without a cancellation receipt

    Examples:
      | state      |
      | off        |
      | retired    |
      | unadmitted |
      | lost       |
      | stale      |
      | sleeping   |

  @idle_veto
  Scenario: Forced sleep is always allowed and a valid wake can retain idle protection
    Given admitted No Sleep with controlled OS sleep replies
    When the owner suspends and restores admission across forced sleep and wake
    Then only the impending sleep was acknowledged without veto
    When the controlled OS asks to idle sleep with token 714
    Then only that token is cancelled and its exact acquisition receipt preserves both assertions

  @idle_veto
  Scenario: A failed idle cancellation is error-first and never produces a success receipt
    Given admitted No Sleep with controlled OS sleep replies
    When the controlled idle cancellation fails for token 715
    Then its exact API error precedes veto failure without a cancelled or new On receipt

  Scenario: Real sleep notifications continue the active native operation
    Given an enabled native owner with controlled sleep notifications
    When the source reports impending sleep and completed wake
    Then both typed interruptions carry the exact active operation count and clocks
    And only the impending sleep was acknowledged without veto

  Scenario: Idle-sleep permission is allowed without inventing an interruption
    Given an enabled native owner with controlled sleep notifications
    When the source asks permission to sleep then reports completed wake
    Then only completed wake is observed and permission was allowed

  Scenario: Off and replaced native operations do not donate stale context
    Given an enabled native owner with controlled sleep notifications
    When sleep is reported while off before a new enable and wake
    Then only completed wake carries the replacement active operation

  Scenario: Expired weak owner context does not keep the bus alive
    Given an enabled native owner with controlled sleep notifications
    When the registry is disposed before a sleep notification
    Then no interruption is fabricated and native ownership is acyclic

  Scenario: A retained partial acquisition reports its real resource count
    Given a partially acquired native owner with controlled sleep notifications
    When the source reports impending sleep and completed wake
    Then both interruptions truthfully report one retained resource

  Scenario: Bus observers can request explicit Off during a sleep interruption
    Given an enabled native owner with controlled sleep notifications
    When an interruption observer requests explicit Off
    Then resource cleanup completes without blocking acknowledgement

  Scenario: A slow bus observer never delays the OS sleep acknowledgement
    Given an enabled native owner with controlled sleep notifications
    When a wake observer pauses delivery while a new sleep notification arrives
    Then sleep was promptly allowed before that observer resumed

  Scenario: Reentrant monitor stop never joins its own delivery thread
    Given an enabled native owner with controlled sleep notifications
    When a wake observer stops the monitor and a late sleep callback arrives
    Then the source is removed once and the late callback only allows sleep

  Scenario: Repeated explicit start and stop releases every registration
    Given an enabled native owner with controlled sleep notifications
    When the monitor is stopped and restarted twice
    Then all three registrations are removed exactly once

  Scenario: Notification registration failure is explicit
    Given an enabled native owner with controlled sleep notifications
    When a new notification registration fails
    Then startup returns the native registration fault

  Scenario: Invalid occurrence clocks produce an error instead of an interruption
    Given an enabled native owner with controlled sleep notifications
    When the occurrence clock fails during an impending sleep notification
    Then an attributed lifecycle timing fault is observed without an interruption

  Scenario: An OS allowance failure is logged before its observed interruption
    Given an enabled native owner with controlled sleep notifications
    When the OS allowance fails for an impending sleep notification
    Then the allowance error precedes the typed interruption

  Scenario: Slow external observers cannot cause an unbounded monitor stop
    Given an enabled native owner with controlled sleep notifications
    When an external observer exceeds the monitor stop deadline
    Then stop fails explicitly within its budget and succeeds after observer release

  @real_host
  Scenario: Actual macOS notification registration can be started and stopped
    When actual macOS sleep monitoring is started and stopped twice
    Then both real registrations completed bounded explicit teardown
  @late_review
  Scenario: Failed sleep observation fences new acquisition but preserves cleanup and restart
    Given an enabled native owner with controlled sleep notifications
    When the sleep source fails before explicit Off and another enable
    Then native acquisition is refused until a new source starts and cleanup stays available
# end lifecycle.feature
