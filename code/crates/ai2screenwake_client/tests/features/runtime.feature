# runtime.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Shared-owner request and episode isolation
  Scenario: Concurrent queries retain both observations when native replies in reverse order
    Given a confirmed On shared owner
    When two independent queries are sampled in reverse order
    Then both queries publish an ordered observed snapshot
# end runtime.feature
