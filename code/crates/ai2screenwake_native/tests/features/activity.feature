# activity.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Genuine native idle observations
  Scenario: Valid idle samples preserve request identity and paired clocks
    Given a native owner with controlled OS calls
    When native input age is sampled
    Then 1234 milliseconds and the original request clocks are observed

  Scenario Outline: Invalid input age <age> is an explicit null observation
    Given a native owner reporting input age <age>
    When native input age is sampled
    Then an attributed idle error precedes a null observation

    Examples:
      | age                  |
      | -1                   |
      | NaN                  |
      | inf                  |
      | -inf                 |
      | 18446744073709552    |

  Scenario: An OS sampling fault never becomes a zero idle age
    Given a native owner whose input source fails
    When native input age is sampled
    Then an attributed idle error precedes a null observation

  Scenario: A clock failure never fabricates an occurrence timestamp
    Given a native owner whose clock fails
    When native input age is sampled
    Then a timing error is observed without a fabricated sample

  @real_host
  Scenario: App and child client resolve the same OS login runtime location
    When two processes resolve the native runtime location
    Then both paths agree without using the TMPDIR override

  @real_host
  Scenario: Invalid owner paths fail before any application launch
    When invalid owner bundle paths are submitted
    Then each launch is explicitly refused
# end activity.feature
