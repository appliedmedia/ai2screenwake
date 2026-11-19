# build.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Canonical native application build
  Packaging admits only the documented release configuration and preserves complete outputs on failure.

  @daily_version
  Scenario Outline: An owned successful publication selects the next local daily version
    Given an owned signed application with published version "<previous>"
    When the build freezes local date "2026.11.18"
    Then every producer and native metadata use frozen version "<expected>"

    Examples:
      | previous       | expected        |
      | legacy         | v2026.11.18(1)   |
      | 2026.11.18:3    | v2026.11.18(4)   |
      | 2026.11.17:9    | v2026.11.18(1)   |

  @daily_version
  Scenario: Failed versioned publication does not consume a successful revision
    Given an owned signed application with published version "2026.11.18:1"
    When a versioned publication fails before complete staging
    Then the exact previous app remains and its next daily revision is still two

  @daily_version
  Scenario: Only successful atomic publication advances the next revision
    Given an owned signed application with published version "2026.11.18:1"
    When the complete version-two app is atomically published
    Then the next publication selects version three without changing the prior directory

  @daily_version
  Scenario Outline: Invalid previous version evidence cannot allocate a revision
    Given an owned signed application with published version "<previous>"
    When the build freezes local date "2026.11.18"
    Then version allocation fails without changing the previous application

    Examples:
      | previous        |
      | 2026.11.18:9999  |
      | 2026.11.19:1     |
      | malformed       |

  @daily_version
  Scenario: A same-source compiler artifact from another revision is rejected before snapshot replacement
    Given a captured current FFI producer in a shared compiler cache
    When its compiler bytes carry a different frozen daily version
    Then producer validation fails without relabeling the previous snapshots

  @bundle_upgrade
  Scenario Outline: Atomic publication preserves a verified <previous> predecessor until complete replacement
    Given an isolated build directory with a previous application
    When the complete current app replaces a signed <previous> predecessor
    Then the signed predecessor is atomically exchanged and unrelated output is preserved
    Examples:
      | previous |
      | fifteen  |
      | nineteen |

  @bundle_upgrade
  Scenario Outline: A signed predecessor with <defect> is not overwritten
    Given an isolated build directory with a previous application
    When the complete current app encounters a predecessor with <defect>
    Then publication preserves the exact refused predecessor and unrelated output
    Examples:
      | defect   |
      | extra    |
      | tampered |
      | foreign  |

  @app_naming
  Scenario: The renamed application publishes only its paired executable and preview resources
    Given an isolated build directory with a previous application
    When the complete ai2sw application is published
    Then only the canonical ai2sw target contains the new executable and preview resources

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
  @compiler_identity
  Scenario: Shared cache feature alternation cannot overwrite captured Swift inputs
    Given a captured current FFI producer in a shared compiler cache
    When MCP overwrites the shared library and cached FFI is selected again
    Then Swift inputs retain only the verified FFI bytes and prior app remains untouched

  @compiler_identity
  Scenario Outline: A <defect> producer cannot be substituted for current FFI
    Given a captured current FFI producer in a shared compiler cache
    When a <defect> compiler receipt is offered as current FFI
    Then producer validation fails without relabeling the previous snapshots

    Examples:
      | defect     |
      | MCP        |
      | debug      |
      | foreign    |
      | unfinished |
      | failed     |

  @compiler_identity
  Scenario: Shared compiler storage does not unify production feature profiles
    Given an isolated build directory with a previous application
    When the three production Cargo producers are planned
    Then feature profiles remain separate while compiler units share only this worktree target
  @compiler_identity
  Scenario Outline: An independent stage that <outcome> cannot leave its compiler behind
    Given an isolated build directory with a previous application
    When an independent build slice <outcome> beside its owned compiler
    Then both owned stages have finished before any application publication

    Examples:
      | outcome   |
      | completes |
      | fails     |
  @late_review
  Scenario: A reparented new-group writer cannot survive early stage failure
    Given an isolated build directory with a previous application
    When a orphan build stage leaves a controlled descendant writer
    Then the failed stage has no live parent descendant or late output mutation

  @late_review
  Scenario: Nested build failure retains ownership of a reparented new-group writer
    Given an isolated build directory with a previous application
    When a nestedorphan build stage leaves a controlled descendant writer
    Then the failed stage has no live parent descendant or late output mutation

  @late_review
  Scenario: Caller death closes the supervisor descriptor and releases its new-group writer
    Given an isolated build directory with a previous application
    When the stage caller dies after its new-group writer is ready
    Then the failed stage has no live parent descendant or late output mutation

  @late_review
  Scenario: An actual Cargo exec chain cannot outlive its outer stage timeout
    Given an isolated build directory with a previous application
    When Cargo launches a nested stage before its readiness-bounded outer timeout
    Then the failed stage has no live parent descendant or late output mutation
# end build.feature
