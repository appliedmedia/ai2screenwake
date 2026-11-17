<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# AI2ScreenWake artwork

* [app.svg](<app.svg>): detailed 1024 x 1024 transparent master for the app icon and promotional artwork. An amber retro cabinet, mint screen, antennas, smile, closed eyes, and rising zzZ establish the identity.
* [menu-off.svg](<menu-off.svg>): 24 x 24 transparent monochrome master with large closed eyes.
* [menu-on.svg](<menu-on.svg>): matching 24 x 24 master with very wide-open eyes and no sleep marks.

The menu pair shares a simple cabinet, antenna and small smile; miniature controls, feet and sleep marks no longer crowd the eyes. Large closed versus wide-open eyes distinguish state without color. Black artwork and transparent negative space support native template tinting in light and dark appearances; do not add an opaque white background. Rust selects both icon and preview from observed protection, never a pending click.

`cargo xtask assets` exports the same two menu masters as `menu-{off,on}{,@2x}.png` at 24/48 pixels and `preview-{off,on}{,@2x}.png` at 96/192 pixels under `target/ai2sw/assets`. The 96-point popover preview is independently rasterized, not an enlarged small bitmap. The exporter checks all nineteen outputs, their source hashes, dimensions and black/alpha template pixels. The detailed app icon remains unchanged apart from its product title. These original masters use no fonts, external images or runtime SVG rendering; generated assets alone are not app acceptance.

<!-- end README.md -->
