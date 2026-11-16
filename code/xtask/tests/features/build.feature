# build.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Canonical native application build
  Packaging admits only the documented release configuration and preserves complete outputs on failure.

  Scenario: Unsupported configurations fail before compilation
    When the build command requests the debug profile
    Then it reports that the release profile is required

  Scenario: Absent producer source preserves existing outputs
    Given an isolated build directory with a previous application
    When the build runs without native producer sources
    Then the missing production source is named and existing outputs are unchanged

  Scenario: Changed artifacts cannot be consumed as current build inputs
    Given an isolated build directory with a previous application
    When a recorded build input changes
    Then verification rejects its stale byte identity

  Scenario: Partial staging never replaces an existing application
    Given an isolated build directory with a previous application
    When publication receives an incomplete staged application
    Then publication fails and existing outputs are unchanged

  Scenario: Failed child commands retain failed-stage evidence
    Given an isolated build directory with a previous application
    When a build stage executes an unsuccessful command
    Then its failure and timing are retained without a success result

  Scenario: Symlinked build targets cannot escape the assigned worktree
    Given an isolated build directory with a previous application
    When a build target points through a symlink
    Then path validation rejects the target without changing its destination

  @owned_descendants
  Scenario Outline: A <mode> stage removes its exact descendant before returning
    Given an isolated build directory with a previous application
    When a <mode> build stage leaves a controlled descendant writer
    Then the failed stage has no live parent descendant or late output mutation

    Examples:
      | mode    |
      | failed  |
      | timeout |
      | nested  |
      | outertimeout |
# end build.feature
