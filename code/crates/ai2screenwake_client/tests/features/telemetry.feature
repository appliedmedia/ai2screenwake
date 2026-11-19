# telemetry.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Explicit private owner Actions forwarding
  @review_fix
  Scenario: A long checkout uses a private short-path owner without weakening socket validation
    Given a real private owner whose checkout cannot fit a Unix socket
    When the owner is queried through Actions
    Then inactive forwarding has no attention fault or delivery claim and protection stays Off

  @inactive
  Scenario: Unconfigured analytics remains inactive without a false configuration warning
    Given a real private owner without telemetry configuration
    When the owner is queried through Actions
    Then inactive forwarding has no attention fault or delivery claim and protection stays Off

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
    When 80 owner queries overflow forwarding and Quit cleans up during a fresh held request
    Then overflow and abandonment are explicit and the owned forwarding worker stops

  Scenario: Fatal reporting is independently authorized without an analytics destination
    Given only unrecoverable reports are authorized for the loopback collector
    When a private unrecoverable source error is dispatched
    Then exactly the redacted fatal report is accepted and missing analytics stays explicit

  Scenario: A failed unrecoverable report is never confirmed as delivered
    Given the report-only collector rejects delivery
    When a private unrecoverable source error is dispatched
    Then the fatal report failure is explicit without false delivery or power disruption

  @inactive
  Scenario: Explicitly disabled forwarding stays inactive without attention or successful delivery
    Given forwarding is explicitly disabled beside an owned collector
    When the owner is queried through Actions
    Then disabled forwarding is explicit without a post or delivery confirmation

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
