<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake artwork

* [app.svg](<app.svg>): detailed 1024 x 1024 transparent master for the app icon and promotional artwork. An amber retro cabinet, mint screen, antennas, smile, closed eyes, and rising zzZ establish the identity.
* [menu-off.svg](<menu-off.svg>): 24 x 24 transparent monochrome master with closed eyes, a smile and three ascending vector sleep marks at the upper right.
* [menu-on.svg](<menu-on.svg>): matching master with larger wide-open eyeballs, distinct pupils, one straight horizontal mouth line and no sleep marks.
* [menu-error.svg](<menu-error.svg>): matching attention master with X X eyes and a frown, never sleep marks or an exclamation badge.

All three faces keep the landscape cabinet, inset screen and shared left antenna mount `(6,7.5)`. The horizontal antenna base is removed; shorter tips move down/in to `(4,5.3)` and `(7.6,4.7)`, retaining both visible 1.4-stroke rays. Cabinet/inset outlines remain one pixel. Screen left edge 5.5/right edge 21, both 0.7-radius selector dots and Off sleep marks stay unchanged. Smallest-z/cabinet clearance remains 1.05 logical pixels, with one/two/four/eight fully transparent rows at 24/48/96/192 pixels.

On keeps 2.4/2.8-radius eyes, 1.1 outlines, distinct 0.85-radius pupils and the single 0.8-stroke mouth from `(9,18.5)` to `(17,18.5)`. Closed/smiling, wide/straight and crossed/frowning faces distinguish Off, On and Error without color. Rust supplies visible `Mode: Can Sleep`, `Mode: No Sleep` or `Off: Error`, retaining face/value through truthful accessible Pending; literal `Off: Error` does not replace independently observed protection truth. The large preview is a real observed-power cycle control; its intentional Error preview requires confirmed Off and never fabricates a fault. Template tinting uses black/alpha negative space, never opaque backgrounds.

`cargo xtask assets` exports these masters as `menu-{off,on,error}{,@2x}.png` at 24/48 pixels and `preview-{off,on,error}{,@2x}.png` at 96/192 pixels under `target/ai2sw/assets`. Every density is independently rasterized from the same state master, not enlarged from a bitmap. Four source masters yield twenty-three encoded outputs, including the unchanged detailed app icon. `CompactEarsAndEqualCabinetBorders`, `AwakeEyesAndSingleLineMouth`, and `SmallestSleepMarkClearsCabinet` pair effective geometry with actual raster silhouette, border ink, continuous mouth/negative space, pupil separation and complete clear-row measurements. `CompactSelectorsRemainTwoIsolatedDots` preserves disconnected controls and their screen gap; `TVScreenLeftEarsSleepRightNoErrorPunctuation` preserves closed-smile/wide-straight/crossed-frown and Off-only sleep proofs. Existing inventory/density/black-alpha/provenance/publication checks remain. White review composites are ignored diagnostics only. No fonts, external images or runtime SVG rendering are used; generated assets alone are not physical app acceptance.

<!-- end README.md -->
