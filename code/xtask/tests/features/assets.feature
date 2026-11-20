# assets.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Native artwork export
  @polish_icon
  Scenario: Apple's decoder preserves every generated application icon representation
    Given the canonical ScreenWake artwork masters
    And a complete current asset export
    Then Apple-decoded ICNS pixels match every source PNG within two channel levels

  @polish_art
  Scenario: The detailed TV has left controls and a left-mounted antenna without mirroring its face
    Given the canonical ScreenWake artwork masters
    And a complete current asset export
    Then the control panel is left of the screen and the antenna base is in the cabinet's left half

  @antenna_fit
  Scenario: Round antenna strokes stay above the cabinet interior at every native density
    Given the canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then the complete antenna stroke and antialiasing stay above the cabinet interior

  @provider_initial
  Scenario: Equivalent local and ancestor antenna transforms preserve exported pixels
    Given the canonical ScreenWake artwork masters
    And a complete current asset export
    Then equivalent transformed antenna geometry passes the real raster requirements

  @provider_initial
  Scenario: Both antenna-base mutants are rejected without changing shipped artwork
    Given the canonical ScreenWake artwork masters
    And a complete current asset export
    Then isolated left and right antenna base mutants fail the real raster requirements

  @sleep_menu
  Scenario: ShortRaysWithoutAntennaBase
    Given the canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then all densities show lower short antenna rays without a horizontal base

  @artwork_polish
  Scenario: CompactEarsAndEqualCabinetBorders
    Given the canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then all densities retain smaller paired left ears and equally weighted cabinet and screen borders

  @artwork_polish
  Scenario: AwakeEyesAndSingleLineMouth
    Given the canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then the awake face has larger contained eyeballs with pupils and one continuous horizontal mouth

  @artwork_polish
  Scenario: SmallestSleepMarkClearsCabinet
    Given the canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then the three ascending sleep marks leave a full logical pixel above the cabinet

  @compact_tv @selector_pixels
  Scenario: CrispSelectorPixelsKeepOneColumnGaps
    Given the canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then all native densities distinguish two left selectors from the inset screen border

  @tv_faces
  Scenario: TVScreenLeftEarsSleepRightNoErrorPunctuation
    Given the canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then rendered TV faces have inset screens and left ears with sleep marks only at the Off upper right

  Scenario: Approved vector masters produce native application and menu assets
    Given the canonical ScreenWake artwork masters
    When the native asset exporter runs
    Then it produces the complete native asset set and provenance manifest

  Scenario Outline: Invalid masters cannot publish a success manifest
    Given the canonical ScreenWake artwork masters
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
    Given the canonical ScreenWake artwork masters
    And a complete current asset export
    When a valid source changes after export
    Then current verification rejects stale sources
    When the native asset exporter runs
    Then it produces the complete native asset set and provenance manifest

  Scenario: Changed commit identity cannot reuse an old asset manifest
    Given the canonical ScreenWake artwork masters
    And a complete current asset export
    When the fixture advances to a new commit
    Then current verification rejects stale commit identity

  Scenario: Extra outputs are rejected rather than silently discarded
    Given the canonical ScreenWake artwork masters
    And a complete current asset export
    When an unexpected output appears
    Then verification rejects the extra output and regeneration preserves it

  Scenario: A failed replacement retains the previous complete export
    Given the canonical ScreenWake artwork masters
    And a complete current asset export
    And the "malformed" master defect
    When the native asset exporter runs
    Then export fails without changing the previous manifest
# end assets.feature
