# power.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Native resource ownership through Actions
  @corner_guard
  Scenario: Denied manual-corner permission cannot report idle-only protection as On
    Given a native owner whose manual-corner permission is denied
    When native protection is enabled
    Then an attributed corner failure retains 0 resources

  @corner_guard
  Scenario: A nil input source cannot report idle-only protection as On
    Given a native owner whose corner source installation fails
    When native protection is enabled
    Then an attributed corner failure retains 0 resources

  @corner_guard
  Scenario: Explicit retry and Off do not repeatedly request permission
    Given a native owner whose manual-corner permission is denied
    When native protection is enabled twice
    And native protection is disabled
    Then corner permission was requested only once and no source was installed

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
