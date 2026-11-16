# telemetry.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Explicit private owner Actions forwarding
  Scenario: Unconfigured analytics remains explicit without changing protection
    Given a real private owner without telemetry configuration
    When the owner is queried through Actions
    Then unconfigured forwarding is reported before the unchanged Off settlement

  Scenario: Authorized SDK metadata and unrecoverable reports contain only the allowlist
    Given a real owner authorized only for an owned loopback collector
    When controls and private recoverable and unrecoverable faults are dispatched
    Then the real SDK forwards redacted analytics and only the unrecoverable bug report

  Scenario: Failed HTTP delivery is explicit and nonrecursive without changing protection
    Given an authorized collector returning retryable failures
    When the owner is queried through Actions
    Then delivery fails explicitly without false confirmation or recursive posts

  Scenario: Blocked delivery has bounded overload and does not hold native cleanup
    Given an authorized collector holding its response
    When many owner queries overflow forwarding and Quit confirms cleanup
    Then overflow and abandonment are explicit and the owned forwarding worker stops

  Scenario Outline: Invalid destinations never construct a sending adapter
    Given an owner with invalid authorized configuration "<kind>"
    When the owner is queried through Actions
    Then invalid forwarding is explicit without a post or changed protection
    Examples:
      | kind |
      | blank_key |
      | missing_host |
      | embedded_credentials |
      | query |
      | insecure_remote |
      | wrong_report_path |
# end telemetry.feature
