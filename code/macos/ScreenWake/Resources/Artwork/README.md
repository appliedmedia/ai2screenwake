<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake artwork

* [app.svg](<app.svg>): detailed 1024 x 1024 transparent master for the app icon and promotional artwork. An amber retro cabinet, mint screen, antennas, smile, closed eyes, and rising zzZ establish the identity.
* [menu-off.svg](<menu-off.svg>): 24 x 24 transparent monochrome master with closed eyes, a smile and three ascending vector sleep marks at the upper right.
* [menu-on.svg](<menu-on.svg>): matching master with very wide-open eyes, straight gritted teeth and no sleep marks.
* [menu-error.svg](<menu-error.svg>): matching attention master with X X eyes and a frown, never sleep marks or an exclamation badge.

All three faces share a wider landscape cabinet, a distinct inset screen and both rabbit ears mounted at the upper left. Closed/smiling, wide/gritted and crossed/frowning faces distinguish ordinary Off, confirmed On and attention without color. Black artwork and transparent negative space support native template tinting; do not add an opaque white background. Rust selects the same observed face for icon and preview, never a pending click. Attention uses exactly `Status: Error`; it preserves independently observed protection in accessibility without mutating resources or MCP truth. Pending retains dots.

`cargo xtask assets` exports these masters as `menu-{off,on,error}{,@2x}.png` at 24/48 pixels and `preview-{off,on,error}{,@2x}.png` at 96/192 pixels under `target/ai2sw/assets`. Every density is independently rasterized from the same state master, not enlarged from a bitmap. Four source masters yield twenty-three encoded outputs, including the unchanged detailed app icon. Executable `TVScreenLeftEarsSleepRightNoErrorPunctuation` checks actual exported screen/ear/sleep regions and face/mouth pixels; existing inventory/density/black-alpha checks cover all three states. No fonts, external images or runtime SVG rendering are used; generated assets alone are not physical app acceptance.

<!-- end README.md -->
