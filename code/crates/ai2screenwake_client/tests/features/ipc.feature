# ipc.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Private framed shared-owner transport
  Scenario: Oversized frames are rejected before writing to a peer
    When an envelope exceeds the 64 KiB frame limit
    Then framing returns an explicit oversized-frame error

  Scenario: Two real socket clients and native control share one owner truth
    Given a real private owner socket
    When two socket clients interleave On query and native Off
    Then their correlated results agree with the shared native owner

  Scenario: Real socket peer credentials reject a different expected user
    When a real socket pair is checked against its actual and a different UID
    Then only the actual peer UID is accepted

  Scenario: A second owner cannot replace the live private endpoint
    Given a real private owner socket
    When another owner attempts the same login-session endpoint
    Then the live socket identity and private modes are preserved

  Scenario: Launch-or-connect starts the actual test owner without enabling protection
    Given a private location without an owner
    When a query launches and connects to the real test owner
    Then the first observed owner status is Off and launch happened once
# end ipc.feature
