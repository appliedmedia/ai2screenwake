# assets.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Native artwork export
  Scenario: Approved vector masters produce native application and menu assets
    Given the three canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then it produces the complete native asset set and provenance manifest

  Scenario Outline: Invalid masters cannot publish a success manifest
    Given the three canonical ScreenWake artwork masters
    And the "<defect>" master defect
    When the native asset exporter runs
    Then export fails with "<message>" and no published manifest

    Examples:
      | defect     | message            |
      | missing    | cannot read master |
      | malformed  | invalid master     |
      | external   | unsafe SVG element |
      | dimensions | invalid artwork dimensions |

  Scenario: Changed source bytes invalidate old assets and require regeneration
    Given the three canonical ScreenWake artwork masters
    And a complete current asset export
    When a valid source changes after export
    Then current verification rejects stale sources
    When the native asset exporter runs
    Then it produces the complete native asset set and provenance manifest

  Scenario: Changed commit identity cannot reuse an old asset manifest
    Given the three canonical ScreenWake artwork masters
    And a complete current asset export
    When the fixture advances to a new commit
    Then current verification rejects stale commit identity

  Scenario: Extra outputs are rejected rather than silently discarded
    Given the three canonical ScreenWake artwork masters
    And a complete current asset export
    When an unexpected output appears
    Then verification rejects the extra output and regeneration preserves it

  Scenario: A failed replacement retains the previous complete export
    Given the three canonical ScreenWake artwork masters
    And a complete current asset export
    And the "malformed" master defect
    When the native asset exporter runs
    Then export fails without changing the previous manifest
# end assets.feature
