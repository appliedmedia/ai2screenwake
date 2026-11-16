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

  Scenario: Malformed JSON produces a canonical error on the real socket
    Given a real private owner socket
    When malformed JSON is written to a real peer
    Then the peer receives an explicit invalid-frame error

  Scenario: An oversized incoming length is rejected without waiting for its body
    Given a real private owner socket
    When an oversized length prefix is written without a body
    Then the peer receives an explicit oversized-frame error

  Scenario: A socket client cannot inject a native resource result
    Given a real private owner socket
    When a native outcome is replayed through a real peer
    Then it is rejected without another native operation

  Scenario: Disconnect does not cancel already admitted protection intent
    Given a real private owner socket
    When a caller disconnects before its admitted On operation completes
    Then a new client observes the completed On intent

  Scenario: One client cannot retain more than 32 pending requests
    Given a real private owner socket
    When one client emits 33 setters before native completion
    Then exactly 32 await convergence and the extra request has an explicit error

  Scenario: An owned inactive socket is removed only after exclusive stale checks
    Given a private location without an owner
    When an owned inactive socket is present at startup
    Then a real client can query the replacement Off owner

  Scenario: A socket symlink is never treated as removable stale residue
    Given a private location without an owner
    When a socket symlink points at an unrelated test file
    Then owner startup rejects it and preserves the exact file and link

  Scenario: Fragmented headers and bodies are reconstructed on a real peer
    Given a real private owner socket
    When a valid query is written in separate header and body fragments
    Then the peer receives its matching Off settlement

  Scenario: An existing public runtime directory is not silently repaired or used
    Given a private location without an owner
    When the declared runtime directory has public permissions
    Then the query receives an explicit unsafe-runtime error

  Scenario: Confirmed Quit fences late MCP enable and native Toggle before disposal
    Given a real private owner socket
    When Quit confirms Off and late MCP On and native Toggle are attempted
    Then both late enables have explicit closing errors without acquiring resources

  Scenario: Failed Quit cleanup retains the owner lease and supports explicit Off retry
    Given a real private owner socket
    When native cleanup fails and its transport owner is disposed
    Then a second owner is refused until the retained owner confirms Off cleanup

  Scenario: Durable diagnostic history survives restart without restoring live protection
    Given a real private owner socket
    When verified On history is closed and the same owner location restarts
    Then history remains visible while startup is Off and a new On has no old events

  Scenario: Recent log observation is bounded and explicitly marks omitted history
    Given a real private owner socket
    When many ordered observations exceed the recent log window
    Then the log view is bounded and explicitly reports earlier entries omitted

  Scenario: Corrupt durable history is preserved and rejected before native work
    Given a private location without an owner
    When the durable journal contains a malformed record at startup
    Then startup reports journal corruption without changing those bytes or enabling protection

  Scenario: Journal rotation retains bounded checked history across restart
    Given a real private owner socket
    When repeated native log views rotate the retained journal
    Then both retained files are bounded and restart reads their contiguous diagnostic tail

  Scenario: Rejected private request values never enter durable diagnostics
    Given a real private owner socket
    When a real peer sends an unexpected private value in a control request
    Then its error and durable files omit that private value
# end ipc.feature
