# ui.feature Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
Feature: Generated native menu follows observed owner state
  @provider_current
  Scenario: Saturated native setup completion retains the newest session and its first outcome
    When setup completions arrive out of order while native input is saturated
    Then the newest session completes once with its original outcome

  @provider_current
  Scenario: An old same-acquisition filter loss cannot discard replacement success
    Given a controlled Accessibility setup owner that is On
    When the explicit setup effect completes and actual trust becomes enabled
    And the old filter loses after recheck admission before replacement readiness
    Then replacement readiness survives the old loss and current loss still clears it

  @provider_current
  Scenario: A Recheck whose own journal append fails cannot activate a native filter
    Given a controlled Accessibility setup owner with a Recheck-failing journal
    When the explicit setup effect completes and actual trust becomes enabled
    Then failed Recheck persistence prevents filter activation without skipping basic cleanup

  @setup_review @accessibility_setup
  Scenario: Lifecycle loss invalidates ready setup even when fresh trust remains granted
    Given a controlled Accessibility setup owner that is On
    When the explicit setup effect completes and actual trust becomes enabled
    And the setup acquisition loses lifecycle availability while trust stays enabled
    Then setup readiness stays false without assertion churn or restoring stale authority

  @setup_review @accessibility_setup
  Scenario Outline: Rust schedules permission observations only until explicit <control>
    Given a controlled Accessibility setup owner that is On
    When the explicit setup effect completes and actual trust becomes enabled
    And owner-scheduled permission checks are observed before <control>
    Then no later native permission check occurs after setup retirement

    Examples:
      | control |
      | close   |
      | Off     |
      | Quit    |

  @accessibility_setup @setup_effect_retirement
  Scenario: Off admitted before queued setup rendering prevents the native Settings launch
    Given a controlled Accessibility setup owner that is On
    When explicit Off is queued before the setup effect is delivered
    Then retired setup rendering requests no native window or Settings launch

  @accessibility_setup
  Scenario Outline: Explicit trust setup preserves ordinary <state> protection without assertion churn
    Given a controlled Accessibility setup owner that is <state>
    When the explicit setup effect completes and actual trust becomes enabled
    Then only the current setup reports trust and optional readiness without changing ordinary protection

    Examples:
      | state |
      | On    |
      | Off   |

  @accessibility_setup
  Scenario: System Settings refusal is visible only in the requested setup window
    Given a controlled Accessibility setup owner that is On
    When the explicit Accessibility settings effect is refused
    Then its named nonfatal failure leaves basic protection On and the setup window visible

  @accessibility_setup
  Scenario Outline: A queued <control> retires setup before a stale grant can arm the filter
    Given a controlled Accessibility setup owner that is On
    When <control> precedes the queued native trust observation
    Then the stale setup cannot open settings or arm optional filtering

    Examples:
      | control |
      | close   |
      | Off     |
      | Toggle  |
      | Quit    |

  @accessibility_setup
  Scenario Outline: An admitted <control> fences an already queued optional native recheck
    Given a controlled Accessibility setup owner that is On
    When <control> precedes the queued native filter recheck
    Then the stale setup cannot open settings or arm optional filtering

    Examples:
      | control |
      | close   |
      | Off     |
      | Toggle  |
      | Quit    |

  @accessibility_setup
  Scenario: Ordinary menu rendering advertises explicit setup without opening settings
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    Then Accessibility setup is discoverable without a settings effect or ordinary warning

  @permission_independent
  Scenario: Optional permission denial keeps real private control On without adding menu wording
    Given a private menu with a denied optional corner acquisition
    Then basic On and the private query stay successful without a corner notice

  @real_menu_fault
  Scenario: Real private queries retain actionable recovery after a failed native acquisition
    Given a private menu whose current native acquisition fails permission and cleanup
    When the failed owner is queried twice through its real private connection
    Then permission recovery remains specific in the menu tooltip and remote result

  @real_menu_fault
  Scenario: A genuine native failure leaves explicit cleanup and retry reachable
    Given a private menu whose current native acquisition fails permission and cleanup
    When explicit Off and On are retried after that real failure
    Then the controls remain available and actual power changes without hiding the fault

  @real_menu_fault
  Scenario: A new native operation replaces obsolete permission recovery with its own failure
    Given a private menu whose current native acquisition fails permission and cleanup
    When successful cleanup is followed by a different native acquisition failure
    Then current failure recovery does not request the obsolete permission grant

  @review_fix
  Scenario Outline: A completed website effect survives native-channel saturation with <outcome>
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And a native-channel overflow carries website <outcome>
    Then completion recovery preserves attribution and admits the next explicit website request

    Examples:
      | outcome |
      | success |
      | failure |

  @assembled_guard
  Scenario: No Sleep describes the configured manual saver guard without promising a global saver veto
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    Then capability help distinguishes the authorized corner guard from locking forced sleep and automatic screensavers

  @assembled_guard
  Scenario: A denied corner permission supplies the exact target-app recovery without claiming On
    Given a typed native menu observation is registered
    When the owner observes a pending enable intent
    And native corner authorization reports a permission denial
    Then the menu requests target-app Accessibility permission without inventing protection or a global grant

  @menu_update
  Scenario: Ordinary observations carry immutable build and information copy without opening a website
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    Then the footer matches the compiled build and website opening is not requested

  @menu_update
  Scenario Outline: An explicit information activation observes native <outcome> through Actions
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And an explicit information activation completes with <outcome>
    Then its logged native effect and correlated outcome never change protection

    Examples:
      | outcome |
      | success |
      | failure |

  @provider_initial @preview_cycle
  Scenario: Genuine attention never traps the large TV in confirmed On
    Given a real private menu with Unconfigured forwarding
    When the large TV is activated repeatedly after a genuine fault while confirmed On
    Then each real power target alternates while the genuine fault remains visible

  @provider_initial
  Scenario: Failed steps retain held native work for exact-once fixture cleanup
    When an isolated owner fixture unwinds while holding native Off
    Then its held operation completes exactly once before the real Closed observation

  @sleep_menu @preview_cycle @queued_preview
  Scenario: Distinct queued preview roots retain the latest successful Error transition
    Given a real private menu with Unconfigured forwarding
    When two distinct TV activation roots precede both derived Off setters
    Then both successful Off results produce only the latest intentional Error preview

  @sleep_menu @preview_cycle @queued_preview
  Scenario Outline: An admitted ordinary <control> retires queued preview transitions
    Given a real private menu with Unconfigured forwarding
    When an ordinary <control> is queued between distinct TV roots and their setters
    Then later preview-origin setters cannot restore a retired preview

    Examples:
      | control |
      | setter  |
      | toggle  |
      | quit    |

  @sleep_menu @preview_cycle @queued_preview
  Scenario: Preview origin capacity rejects excess input explicitly and recovers after admission
    Given a real private menu with Unconfigured forwarding
    When queued TV activation roots exceed the native input capacity
    Then excess preview input is an explicit fault and a later activation still settles normally

  @sleep_menu
  Scenario: Visible sleep labels retain confirmed protection through Pending
    Given a typed native menu observation is registered
    When confirmed On is followed by Pending retries stale frames and opposite settlement
    Then visible values use sleep words while accessible Pending remains truthful

  @sleep_menu @preview_cycle
  Scenario: PreviewCyclesOnlyAfterObservedPower
    Given a real private menu with Unconfigured forwarding
    When the large TV cycles from confirmed On through Off Error and On
    Then each preview follows its actual correlated power and remote truth stays ordinary

  @sleep_menu @preview_cycle
  Scenario: Repeated activations cannot manufacture unconfirmed intermediate preview faces
    Given a real private menu with Unconfigured forwarding
    When repeated large TV activations wait for native Off
    Then no Error preview appears before the next separately confirmed Off cycle

  @sleep_menu @preview_cycle
  Scenario: PreviewSupersessionPreservesRealFaults
    Given a real private menu with Unconfigured forwarding
    When an ordinary control supersedes a pending Error preview and a genuine fault follows
    Then the stale preview cannot hide the genuine fault or rewrite remote power

  @sleep_menu @preview_cycle
  Scenario: Failed preview cleanup cannot masquerade as an intentional Off error face
    Given a real private menu with Unconfigured forwarding
    When the preview Off target fails with actual retained resources
    Then genuine cleanup error remains distinct from an intentional Error preview

  @sleep_menu @preview_cycle
  Scenario: Quit fences pending and later preview activations
    Given a real private menu with Unconfigured forwarding
    When Quit overtakes a held preview Off and a later TV activation
    Then confirmed cleanup never permits a new native enable or preview control

  @sleep_menu @readable_newest
  Scenario: NewestReadableEntryBelowPath
    Given a real private menu with Unconfigured forwarding
    When the private menu generates a bounded overflowing diagnostic history
    And the private menu owner enables then disables protection
    And the real private menu opens and refreshes its human log
    Then both actual viewer projections use the same exported basename path and bytes
    And actual human rows descend below the path while canonical history still ascends

  @compact_transition
  Scenario: Real private owner Pending remains nullable remotely without a premature Off face
    Given a real private menu with Unconfigured forwarding
    When the real owner requests Off while native confirmation is held
    Then remote Pending remains nullable while the menu retains On until confirmed Off

  @compact_transition
  Scenario Outline: Confirmed <initial> retains its face through Pending retries until observed settlement
    Given a typed native menu observation is registered
    When confirmed <initial> is followed by Pending retries stale frames and opposite settlement
    Then every emitted frame retains truthful status and changes its face only at confirmation

    Examples:
      | initial |
      | Off     |
      | On      |

  @readable_log_name
  Scenario: A successful readable snapshot supplies the actual timestamped viewer basename
    Given a typed native menu observation is registered
    When the existing log query is emitted
    And its readable snapshot supplies a timestamped absolute path
    Then the viewer title is the actual exported basename and the first line is its full path

  @readable_log_name
  Scenario: Real private owner Show Log refreshes its actual timestamped viewer and file together
    Given a real private menu with Unconfigured forwarding
    When the real private menu opens and refreshes its human log
    Then both actual viewer projections use the same exported basename path and bytes

  Scenario: Confirmed owner On is projected through the one rendering observation stream
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    Then the generated observer receives the awake TV and observed On label

  Scenario: Requested On is never an optimistic awake icon
    Given a typed native menu observation is registered
    When the owner observes a pending enable intent
    Then the observed menu remains explicitly pending without a moving punctuation badge

  @tv_faces
  Scenario: Unconfirmed native errors use the X-eyed Error face without punctuation
    Given a typed native menu observation is registered
    When the owner observes unresolved protection
    Then the menu labels Error rather than falsely claiming Off

  @tv_faces
  Scenario: Sampling errors show attention without rewriting confirmed protection truth
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And a native activity fault with private details is dispatched
    Then the menu shows the Error face with observed On detail and no private chain

  @tv_faces
  Scenario Outline: Intentionally <mode> forwarding leaves the real private menu normally On and Off
    Given a real private menu with <mode> forwarding
    When the private menu owner enables then disables protection
    Then both observed faces are ordinary without telemetry attention or delivery claims

    Examples:
      | mode         |
      | Unconfigured |
      | Disabled     |

  @tv_faces
  Scenario: Genuine attention remains Error while a fresh remote query still confirms On
    Given a real private menu with Unconfigured forwarding
    When a genuine fault precedes another remote confirmed On query
    Then the Error face retains the real owner's confirmed On truth and controls

  @telemetry_ui
  Scenario Outline: Telemetry <family> attention preserves actual <phase> and available controls
    Given an observed <phase> native menu
    When the canonical telemetry <family> fault is dispatched
    Then observed <phase> is retained with a safe accessible telemetry error and unchanged controls

    Examples:
      | family        | phase |
      | Configuration | On    |
      | Configuration | Off   |
      | Transport     | On    |
      | Transport     | Off   |
      | Buffer        | On    |
      | Buffer        | Off   |

  @telemetry_ui
  Scenario Outline: Telemetry <family> errors cannot reopen controls during Quit
    Given an observed On native menu
    When native Quit is admitted without a terminal result
    And the canonical telemetry <family> fault is dispatched
    Then telemetry error visibility preserves in-progress Quit and protection truth

    Examples:
      | family        |
      | Configuration |
      | Transport     |
      | Buffer        |

  Scenario: Show Log projects canonical metadata rather than private payloads
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And a native activity fault with private details is dispatched
    And the existing log query is emitted
    Then the auxiliary log is visible with redacted ordered metadata

  Scenario: Dismissing the auxiliary log is idempotent and preserves protection
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And the existing log query is emitted
    And the log is dismissed twice
    Then it is closed once without changing the observed On state

  Scenario: Dismissal wins over a late canonical log reply
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And the existing log query is emitted
    And the log is dismissed twice
    And the old log query emits a late snapshot
    Then the dismissed log stays closed without another activation

  Scenario: An existing log does not steal focus on background owner observations
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And the existing log query is emitted
    And the owner observes confirmed protection On
    Then the log stays visible without a new activation instruction

  @log_health
  Scenario: Omitted canonical history is disclosed rather than presented as complete
    Given a typed native menu observation is registered
    When the canonical log snapshot reports earlier omitted history
    Then the log visibly discloses earlier missing history

  @log_health
  Scenario: Unhealthy canonical history is disclosed without exposing private faults
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And the canonical log snapshot reports a storage fault
    Then the log warns about unsaved history while preserving On and privacy

  Scenario: Dropping the generated observation token releases callbacks
    Given a typed native menu observation is registered
    When the generated observation token is dropped
    And the owner observes confirmed protection On
    Then the foreign observer is no longer retained or invoked

  Scenario: Off observations alone cannot authorize application termination
    Given a typed native menu observation is registered
    When native Quit is admitted without a terminal result
    And the owner observes confirmed Off
    Then Quit remains in progress without a terminal instruction

  Scenario: Failed Quit offers cleanup retry but never allows a new enable
    Given a typed native menu observation is registered
    When the owner observes confirmed protection On
    And native Quit is admitted without a terminal result
    And that Quit settles with a retained-resource error
    Then Quit is retryable without termination or re-enabled primary control

  @real_host @fixture_parent
  Scenario Outline: Assertion-only real IOKit host preserves an ordinary socket budget of <bytes> bytes
    Given an actual generated native host has a missing parent and a <bytes>-byte ordinary socket budget
    When its typed primary toggle is emitted
    Then confirmed On follows pending and the same IPC owner reports On
    When its typed Quit is emitted
    Then confirmed Off precedes terminal rendering and the private endpoint is removed

    Examples:
      | bytes |
      | 94    |
      | 103   |

  @real_host @fixture_parent
  Scenario: Assertion-only real IOKit host creates its missing private fixture parent before admission
    Given an actual generated native host uses a missing fixture parent
    When its typed primary toggle is emitted
    Then confirmed On follows pending and the same IPC owner reports On
    When its typed Quit is emitted
    Then confirmed Off precedes terminal rendering and the private endpoint is removed

  @real_host
  Scenario: Generated preview reaches real IOKit and private IPC with an explicit assertion-only corner boundary
    Given an actual generated native host is observed
    When its typed preview cycles confirmed Off through Error and On
    Then confirmed On follows pending and the same IPC owner reports On
    When its typed Quit is emitted
    Then confirmed Off precedes terminal rendering and the private endpoint is removed

  @real_host @assembled_host
  Scenario: The generated real IOKit host shares private IPC control with an explicit assertion-only corner boundary
    Given an actual generated native host is observed
    When its typed primary toggle is emitted
    Then confirmed On follows pending and the same IPC owner reports On
    When its typed Quit is emitted
    Then confirmed Off precedes terminal rendering and the private endpoint is removed

  @real_host
  Scenario: A second assertion-only real IOKit host fails explicitly without claiming Off
    Given an actual generated native host is observed
    When a second generated host attempts admission
    Then its error observation never claims Off and its Quit leaves the first owner available
  @ordered_projection
  Scenario: Delayed old snapshots and changes cannot replace a newer Off menu
    Given a typed native menu observation is registered
    When newer Off is followed by older On snapshots and changes
    Then the menu never renders the older awake state

  @ordered_projection
  Scenario: Equal-revision observations are not mistaken for older state
    Given a typed native menu observation is registered
    When a same-revision snapshot supplies confirmed On after Pending
    Then the same-revision observation is rendered rather than discarded

  @ordered_projection @causal_projection
  Scenario: Same-chain queued observations retain their own emitted mode
    Given a typed menu with a queued projection source
    When one root queues On Pending Off and stale observations before rendering
    Then each queued render matches its causal observation and confirmed mode

  @ordered_projection @causal_projection
  Scenario: Persistent On completion followed by Off never renders an older revision
    Given a persistent owner queue with a native completion journal barrier
    When Off is queued behind the journalled On completion before derived state drains
    Then rendered protection revisions never regress and menu and remote status agree Off
  @late_review
  Scenario: A queued final socket fault cannot become successful native termination
    When the final journal cleanup fault reaches the native shutdown helper
    Then terminal delivery requires the retained completion and genuine cleanup evidence

  @late_review
  Scenario Outline: Foreign shutdown preserves <gate> evidence across explicit retries
    When the private disposal gate exercises <gate>
    Then terminal delivery requires the retained completion and genuine cleanup evidence

    Examples:
      | gate       |
      | completion |
      | late_error |
      | peer_error |
# end ui.feature
