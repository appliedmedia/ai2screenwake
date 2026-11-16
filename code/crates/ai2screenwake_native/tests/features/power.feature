# power.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Native resource ownership through Actions
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
