# check.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Canonical regression runner
  A check is current-source regression evidence, not a substitute for product acceptance.

  Scenario: Unsupported arguments never start a full suite
    When the check command receives an unsupported argument
    Then it rejects the argument before running any regression family

  Scenario: New maintained targets are included without merging feature profiles
    When Cargo metadata gains a history test target
    Then the plan includes history and keeps native and MCP profiles separate

  Scenario: Failed processes retain their real status and named stage
    Given an isolated runner evidence directory
    When a required stage exits with status seven
    Then the failed stage records status seven and its source identity

  Scenario: Successful processes cannot pass with no selected scenarios
    Given an isolated runner evidence directory
    When a required runner reports zero scenarios
    Then the named gate fails and records the zero-selection failure

  Scenario: Concurrent executor lanes retain separate outputs and timing
    Given an isolated runner evidence directory
    When three isolated executor lanes run together
    Then every lane retains only its own output and passing count

  Scenario: Real native UI cases cannot share the parallel host selection
    When current native UI regression passes are planned
    Then native UI resource cases have a separate serial tag partition

  @executor_cwd
  Scenario Outline: The <executor> executor preserves the <directory> working directory
    Given an isolated runner evidence directory
    When the <executor> executor reads a relative feature with a <directory> working directory
    Then the intended feature is read and the actual working directory is logged

    Examples:
      | executor | directory |
      | check    | package   |
      | check    | default   |
      | build    | package   |
      | build    | default   |

  @late_review
  Scenario Outline: Foundation preparation cannot certify a different source <identity>
    Given an isolated runner evidence directory
    When foundation source <identity> changes after its executable compiles
    Then preparation rejects changed identity without publishing a receipt

    Examples:
      | identity |
      | bytes    |
      | head     |
# end check.feature
