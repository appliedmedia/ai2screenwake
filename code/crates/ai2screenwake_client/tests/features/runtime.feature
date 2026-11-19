# runtime.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Shared-owner request and episode isolation
  @real_menu_fault
  Scenario: The first acquisition fault survives rollback failure and subsequent queries
    Given an owner whose native display acquisition and rollback both fail
    When the actual native acquisition fails and its result is queried twice
    Then every result retains the first actual fault and explicit Off can recover

  @real_menu_fault
  Scenario: An old failure cannot replace the cause of the current native operation
    Given a new shared owner
    When two native acquisitions fail with different causes and the first failure is repeated
    Then subsequent queries retain only the current operation failure

  @review_fix
  Scenario: An obsolete enabled native envelope cannot reacquire after completed Off
    Given a confirmed On shared owner
    When an old enabled native operation is offered after confirmed Off
    Then no obsolete physical work starts and a newer explicit On remains admissible

  @review_fix
  Scenario: Valid wake after corner loss permits a new explicit acquisition without restoring the lost one
    Given a confirmed On shared owner
    When corner loss occurs between valid sleep and wake
    Then the lost request stays failed and a new explicit On receives current authorization

  @corner_guard
  Scenario: Internal corner admission precedes native activation without publishing On early
    Given a new shared owner
    When protection is requested without a native reply
    Then exact corner admission is logged while outward protection remains pending

  @corner_guard
  Scenario: Corner source loss cannot be overwritten by a late native acquisition
    Given a new shared owner
    When corner source loss precedes its acquisition acknowledgement
    Then protection remains unresolved with the newer wake failure

  @corner_guard
  Scenario: Wake cannot restore protection after current corner source loss
    Given a confirmed On shared owner
    When current corner protection is lost before a completed wake
    Then protection remains unresolved with the newer wake failure

  @corner_guard
  Scenario: Retired corner loss cannot invalidate another native acquisition
    Given a confirmed On shared owner
    When an old corner generation reports source loss
    Then stale corner loss leaves current protection and its result unchanged

  @corner_guard
  Scenario: Corner loss during Off does not settle unfinished native cleanup
    Given a confirmed On shared owner
    When corner protection is lost while explicit Off is pending
    Then Off still awaits native zero resources and settles successfully

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

  Scenario: Native Toggle follows the latest owner intent while protection is pending
    Given a new shared owner
    When the native toggle is emitted twice before native completion
    Then typed On and Off intents are logged with only one physical apply pending

  Scenario: Real sleep and wake evidence exclude through-sleep protected-idle time
    Given a confirmed On shared owner
    When native sleep at 5000 and wake at 10000 are observed
    And a fresh query samples idle age 10000 at monotonic time 11000
    Then protected-idle duration is exactly 1 second

  Scenario: Sleep interruption settles every already pending query explicitly
    Given a confirmed On shared owner
    When a query is pending as native sleep is observed
    Then the pending query receives one explicit system-sleep error

  Scenario: Stale native lifecycle identities cannot reset the current owner
    Given a confirmed On shared owner
    When native sleep carries a different operation generation
    Then an attributed stale-interruption error leaves protection On

  Scenario: A Toggle child queued behind Quit cannot resurrect protection
    Given a new shared owner
    When Quit is queued before a Toggle emits its derived setter
    Then the derived enable is rejected and no native resource is acquired

  Scenario: Only a matched eligible request and successful cancellation creates a record
    Given a confirmed On shared owner
    When a matched native cancellation occurs at monotonic time 3000 and epoch 900000
    Then one record preserves that occurrence epoch and episode-relative minutes

  Scenario: Held assertions alone do not prove a suppressed event
    Given a confirmed On shared owner
    When a cancellation receipt has no matching eligible native request
    Then no suppression record is inferred and an explicit unmatched error is logged

  Scenario: Failed native cancellation acknowledgement is not recorded as success
    Given a confirmed On shared owner
    When an eligible native request receives a failed cancellation acknowledgement
    Then the live suppression list stays empty with an explicit cancellation error

  Scenario: The 65th verified record retains only the latest 64 with honest truncation
    Given a confirmed On shared owner
    When 65 matched native cancellations are observed
    Then records two through 65 remain and truncation becomes true only beyond 64

  Scenario: Genuine activity clears live records and rejects late prior-interval receipts
    Given a confirmed On shared owner
    When genuine activity follows a record and another pending cancellation
    Then the new idle interval is empty and the late receipt cannot restore old records

  Scenario: Repeated queries and On requests preserve the active idle interval
    Given a confirmed On shared owner
    When a verified record is followed by repeated queries and On requests
    Then the record and anchor survive while duration advances truthfully

  Scenario: Wall-clock jumps cannot change occurrence-recorded monotonic offsets
    Given a confirmed On shared owner
    When cancellation epochs jump backward while monotonic time advances
    Then event order timestamps and offsets remain tied to each occurrence

  Scenario: Wake starts a fresh live interval without restoring prior suppression records
    Given a confirmed On shared owner
    When a verified record is followed by real sleep wake and a new cancellation
    Then only the post-wake record remains with a post-wake offset

  Scenario: Stale request evidence cannot move the live protected-idle anchor
    Given a confirmed On shared owner
    When replayed request evidence claims a different idle anchor
    Then the verified record and anchor survive that rejected evidence

  Scenario: Unmatched receipts cannot reset a verified live interval
    Given a confirmed On shared owner
    When an unmatched cancellation claims a different idle anchor
    Then the verified record and anchor survive that rejected evidence

  Scenario: An older sample cannot disguise a newer failed idle observation
    Given a confirmed On shared owner
    When the newer of two samples fails before the older one succeeds
    Then both requests retain the explicit latest sampling failure without false Off

  Scenario: A sample cannot move behind already observed genuine activity
    Given a confirmed On shared owner
    When a newer sample contradicts the known activity anchor
    Then its query has an explicit stale-sample error and null duration while On stays true

  Scenario: A failed first durable append cannot enable native protection
    Given an owner whose first durable append fails
    When On is requested through the failing journal
    Then one explicit storage error result is observed and native On was never called

  Scenario: Rejected enabling append still permits explicit zero-resource Quit cleanup
    Given an owner whose enabling-apply append fails
    When On is requested through the failing journal
    And explicit Quit cleanup follows the storage failure
    Then native Off is observed before Closed without a false durable-success result

  Scenario: A failed successful-result append cannot escape as a success acknowledgement
    Given an owner whose success-result append fails
    When On is requested through the failing journal
    Then the caller observes exactly one failed terminal result after its storage error

  Scenario: A log-view append failure cannot claim healthy durable history
    Given an owner whose log-view append fails
    When the native log view is requested through the failing journal
    Then its observed log projection explicitly includes the storage failure

  Scenario: Same-session Off and new On start a fresh protected-idle interval
    Given a confirmed On shared owner
    When a verified record is followed by explicit Off and a newly confirmed On
    Then the new On has no old records or pre-enable idle time
  @apply_clock
  Scenario: A preliminary native clock failure permits an explicit recovered retry
    Given a real native owner whose apply clock 1 fails once
    When On encounters that clock fault
    Then a recovered explicit On succeeds and Quit releases its actual IDs

  @apply_clock
  Scenario Outline: A post-acquisition clock failure permits explicit <control> cleanup
    Given a real native owner whose apply clock 2 fails once
    When On encounters that clock fault
    Then the failed acquisition retains both IDs until explicit <control> cleanup
    And all native failures are error-first without fake clocks or duplicate results

    Examples:
      | control |
      | Off     |
      | Quit    |

  @apply_clock
  Scenario Outline: An Off clock <call> failure cannot skip actual resource release
    Given a real native owner whose apply clock 0 fails once
    When confirmed On is followed by an Off clock <call> fault
    Then all native failures are error-first without fake clocks or duplicate results

    Examples:
      | call |
      | 1    |
      | 2    |
  @apply_clock
  Scenario: A repeated old native failure cannot retire a newer pending cleanup
    Given a new shared owner
    When an old terminal native failure is repeated while explicit Off is pending
    Then only the current native completion settles Off successfully
  @late_review
  Scenario Outline: Saturated pending queries cannot prevent explicit <control> cleanup
    Given a confirmed On shared owner
    When 128 native idle queries are pending before explicit <control>
    Then native release proceeds and all saturated requests settle before confirmed Off

    Examples:
      | control |
      | Off     |
      | Quit    |

  @late_review
  Scenario: An older successful apply cannot erase a newer failed wake
    Given a new shared owner
    When a failed wake precedes delivery of its older successful apply
    Then protection remains unresolved with the newer wake failure

  @late_review
  Scenario: Rejected native On does not leave a toggle target latched after source recovery
    Given a new shared owner
    When native Toggle is rejected during source loss and retried after recovery
    Then one recovered Toggle reaches native On without requiring an extra click

  @late_review
  Scenario: Lifecycle loss cannot prematurely fail an explicit Off cleanup
    Given a confirmed On shared owner
    When lifecycle observation is lost while explicit Off is pending
    Then Off still awaits native zero resources and settles successfully

  @late_review
  Scenario: Normal lifecycle teardown after Quit is not a new protection failure
    Given a confirmed On shared owner
    When lifecycle observation stops after confirmed Quit cleanup
    Then normal teardown adds no failure to the confirmed Off outcome

  @late_review
  Scenario: Lost lifecycle observation rejects On without preventing Off or explicit recovery
    Given a confirmed On shared owner
    When the native source reports unavailable before Off and another On request
    Then only Off reaches native until a newer availability event permits explicit On

  @late_review
  Scenario: One millisecond idle sampling drift preserves the current protected interval
    Given a confirmed On shared owner
    When a verified cancellation is followed by one millisecond anchor jitter
    Then queries and cancellation proofs retain the same interval without stale errors
# end runtime.feature
