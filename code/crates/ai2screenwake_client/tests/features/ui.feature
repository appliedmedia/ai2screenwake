# ui.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Generated native menu follows observed owner state
  Scenario: Confirmed owner On is projected through the one rendering observation stream
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    Then the generated observer receives the awake TV and observed On label
# end ui.feature
