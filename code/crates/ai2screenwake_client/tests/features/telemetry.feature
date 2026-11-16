# telemetry.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Explicit private owner Actions forwarding
  Scenario: Unconfigured analytics remains explicit without changing protection
    Given a real private owner without telemetry configuration
    When the owner is queried through Actions
    Then unconfigured forwarding is reported before the unchanged Off settlement
# end telemetry.feature
