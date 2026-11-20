# power.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Native resource ownership through Actions
  @accessibility_setup
  Scenario: Ordinary denied On and retries never request an Accessibility challenge
    Given a native owner whose manual-corner permission is denied
    When native protection is enabled twice
    Then basic assertions remain held without any permission request
    When native protection is disabled
    Then no resources remain and cleanup was observed

  @permission_independent @corner_guard
  Scenario: Denied optional corner permission preserves basic assertions and explicit cleanup
    Given a native owner whose manual-corner permission is denied
    When native protection is enabled
    Then optional corner failure is logged without failing both basic assertions
    When native protection is disabled
    Then no resources remain and cleanup was observed

  @permission_independent @corner_guard
  Scenario: Unavailable optional corner installation preserves basic assertions
    Given a native owner whose corner source installation fails
    When native protection is enabled
    Then optional corner failure is logged without failing both basic assertions

  @permission_independent
  Scenario: Optional permission denial does not disguise a real assertion acquisition failure
    Given denied optional corners and a failing basic display assertion
    When native protection is enabled
    Then an attributed error precedes a failed observation holding 0 resources

  @permission_independent @real_host
  Scenario: Denied corner permission still acquires and releases actual IOKit assertions
    Given a real IOKit owner with explicitly denied controlled corner permission
    When native protection is enabled
    Then both native assertion IDs have live IOKit properties
    When native protection is disabled
    Then neither native assertion ID has live IOKit properties

  @review_fix
  Scenario: No-tap polling waits without delaying bounded stop responsiveness
    Given an admitted native owner with a controlled saver corner
    Then empty-source polling waits rather than spinning continuously

  @review_fix
  Scenario: A replacement edited after watch attachment cannot adopt an old saver mapping
    Given an admitted native owner with a controlled saver corner
    Then a replacement changed at the watch boundary is rejected before adoption

  @review_fix
  Scenario: Original movement leaving a corner permits a second entry fact
    Given an admitted native owner with a controlled saver corner
    When native protection is enabled
    Then two separate original corner entries produce two attributed deflection facts
    When native protection is disabled
    Then the retired callback passes through and its source stopped once

  @permission_budget
  Scenario: A no-saver interval preserves the first eligible permission request
    Given a native owner with no saver corner and no permission grant
    When native protection is enabled
    And native protection is disabled
    Then the no-saver interval made no permission call
    When the controlled corner selects a saver and native protection is enabled
    Then the first eligible denial attempts permission once before its attributed failure
    When native protection is enabled
    Then the denied permission retry does not attempt another request

  @permission_budget
  Scenario: Configuration failure before permission does not consume its request
    Given a native owner whose corner configuration cannot be read
    When native protection is enabled
    Then optional corner failure is logged without failing both basic assertions
    And the failed configuration made no permission call
    When the controlled corner selects a saver and native protection is enabled
    Then the first eligible denial attempts permission once before its attributed failure
    When native protection is enabled
    Then the denied permission retry does not attempt another request

  @permission_budget
  Scenario: An existing grant does not consume a future permission request
    Given an admitted native owner with a controlled saver corner
    When native protection is enabled
    And native protection is disabled
    Then the trusted interval did not attempt a permission request
    When the controlled permission grant is removed and native protection is enabled
    Then the first eligible denial attempts permission once before its attributed failure
    When native protection is enabled
    Then the denied permission retry does not attempt another request

  @permission_budget
  Scenario: An ineligible caller does not spend the target-app permission request
    Given a native owner whose permission boundary cannot request for the target app
    When native protection is enabled
    Then optional corner failure is logged without failing both basic assertions
    And the ineligible boundary did not attempt a permission request
    When the controlled permission request becomes eligible and native protection is enabled
    Then the first eligible denial attempts permission once before its attributed failure
    When native protection is enabled
    Then the denied permission retry does not attempt another request

  @corner_guard
  Scenario: Explicit retry and Off do not repeatedly request permission
    Given a native owner whose manual-corner permission is denied
    When native protection is enabled twice
    And native protection is disabled
    Then corner permission was requested only once and no source was installed
    When native protection is enabled
    Then the denied permission retry does not attempt another request

  @corner_guard
  Scenario: Only the admitted live acquisition deflects original corner entry
    Given an admitted native owner with a controlled saver corner
    When native protection is enabled
    Then the saver corner is deflected and the lock corner is unchanged
    When native protection is disabled
    Then the retired callback passes through and its source stopped once

  @corner_guard
  Scenario: A disabled tap revokes before its error reaches the bus
    Given an admitted native owner with a controlled saver corner
    When native protection is enabled
    And the corner input source is disabled
    Then the callback passes through and a typed source loss follows its error

  @corner_guard
  Scenario: Old authorization cannot restore a retired corner generation
    Given an admitted native owner with a controlled saver corner
    When native protection is enabled
    And Off is observed before stale corner authorization
    Then the retired callback passes through and its source stopped once

  @corner_guard
  Scenario: Quartz geometry handles negative and portrait displays without scaling input
    Given an admitted native owner with a controlled saver corner
    Then corner geometry preserves ordinary edges modifiers and security corners

  @corner_guard
  Scenario: Real unposted CGEvents cross the original-event deflection boundary
    Given an admitted native owner with a controlled saver corner
    When native protection is enabled
    Then unposted movement and drag events are deflected without changing keys or clicks
    When native protection is disabled
    Then unposted movement passes through the retired callback

  @corner_guard
  Scenario: A toggle while Off does not retire the future On acquisition
    Given an admitted native owner with a controlled saver corner
    When a native toggle is observed while Off
    And native protection is enabled
    Then the saver corner is deflected and the lock corner is unchanged

  @corner_guard
  Scenario: Unadmitted acquisitions cannot install optional filtering or report On
    Given an admitted native owner with a controlled saver corner
    When native resources are requested without corner admission
    Then an attributed corner failure retains 0 resources

  @corner_guard
  Scenario: Failed optional activation does not roll back basic protection
    Given a corner source that loses readiness at final activation
    When native protection is enabled
    Then optional corner failure is logged without failing both basic assertions

  @corner_guard
  Scenario: Quit retires input filtering before native resource cleanup
    Given an admitted native owner with a controlled saver corner
    When native protection is enabled
    And Quit is observed before native cleanup
    Then the retired callback passes through and its source stopped once

  @corner_guard
  Scenario: Dropping the owner tears down its corner source without keeping Actions alive
    Given an admitted native owner with a controlled saver corner
    When native protection is enabled
    And the registry and native owner are dropped
    Then the dropped callback passes through and its source stopped once

  @corner_guard
  Scenario: Unrelated preferences writes preserve a freshly revalidated assignment
    Given an admitted native owner with a controlled saver corner
    Then unrelated files and unrelated Dock settings preserve the exact corner assignment

  @corner_guard
  Scenario: Replacing a saver assignment with Lock never reuses the saved saver mapping
    Given an admitted native owner with a controlled saver corner
    Then an atomic saver to Lock replacement fails configuration revalidation

  @corner_guard
  Scenario: An unsafe replacement cannot be treated as unchanged preferences
    Given an admitted native owner with a controlled saver corner
    Then malformed and symlink preference replacements fail closed

  @corner_guard
  Scenario: Source teardown timeout remains an explicit failure until bounded retry succeeds
    Given a native corner source whose poll is deliberately held
    When native protection is enabled
    And native protection is disabled
    Then an attributed corner failure retains 0 resources
    When the held corner source is released and cleanup is retried
    Then the retired callback passes through and its source stopped once

  Scenario: Enabling acquires both idle resources before reporting success
    Given a native owner with controlled OS calls
    When native protection is enabled
    Then both distinct resources are held with an applied observation

  Scenario: Repeated enable does not duplicate resources
    Given a native owner with controlled OS calls
    When native protection is enabled twice
    Then both distinct resources are held with an applied observation
    And only two acquisitions occurred

  Scenario: Partial acquisition is rolled back before reporting failure
    Given a native owner whose display acquisition fails
    When native protection is enabled
    Then an attributed error precedes a failed observation holding 0 resources

  Scenario: Failed rollback retains its resource for a later off retry
    Given a native owner whose display acquisition and first rollback fail
    When native protection is enabled
    Then an attributed error precedes a failed observation holding 1 resources
    When native protection is disabled
    Then no resources remain and the retained release was retried

  Scenario: Failed release cannot falsely report off
    Given a native owner with controlled OS calls
    When native protection is enabled
    And the next release fails
    And native protection is disabled
    Then an attributed error precedes a failed observation holding 1 resources
    When native protection is disabled
    Then no resources remain and the retained release was retried

  Scenario: Synchronous observers can request cleanup without deadlock
    Given a native owner with an observer that turns off confirmed resources
    When native protection is enabled
    Then no resources remain and cleanup was observed

  Scenario: Dropping the owner releases resources without retaining the bus
    Given a native owner with controlled OS calls
    When native protection is enabled
    And the registry and native owner are dropped
    Then the OS boundary has no retained resources

  @real_host
  Scenario: Actual macOS assertions can be acquired queried sampled and released
    Given a native owner using actual macOS APIs
    When native protection is enabled
    Then both native assertion IDs have live IOKit properties
    When native input age is sampled
    Then a genuine nonnegative input age and current clocks are observed
    When native protection is disabled
    Then neither native assertion ID has live IOKit properties
# end power.feature
