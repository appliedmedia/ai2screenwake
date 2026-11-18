// assets.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Executable requirements at the native asset exporter boundary.

use cucumber::{World, given, then, when};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::Instant,
};

#[path = "../../src/assets.rs"]
mod assets;

// Share only verified setup; each scenario owns independent source, Git and output files.
static COMPLETE_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static COMPLETE_EXPORT: OnceLock<std::result::Result<Vec<u8>, String>> = OnceLock::new();

#[derive(Debug, Default, World)]
struct Artwork {
    fixture: Option<tempfile::TempDir>,
    result: Option<std::result::Result<assets::Manifest, String>>,
    previous_manifest: Option<Vec<u8>>,
}

impl Artwork {
    fn root(&self) -> &Path {
        self.fixture.as_ref().unwrap().path()
    }

    fn target(&self) -> PathBuf {
        self.root().join(assets::TARGET)
    }
}

fn git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .env("GIT_CONFIG_COUNT", "3")
        .env("GIT_CONFIG_KEY_0", "commit.gpgSign")
        .env("GIT_CONFIG_VALUE_0", "true")
        .env("GIT_CONFIG_KEY_1", "gpg.format")
        .env("GIT_CONFIG_VALUE_1", "openpgp")
        .env("GIT_CONFIG_KEY_2", "gpg.program")
        .env("GIT_CONFIG_VALUE_2", "/usr/bin/false")
        .current_dir(root)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fixture Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit(root: &Path) {
    git(
        root,
        &[
            "-c",
            "user.name=AssetTests",
            "-c",
            "user.email=assets@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "Asset fixture",
        ],
    );
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            assert!(kind.is_file(), "fixture copies must not follow symlinks");
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn assert_export_identity(root: &Path, original: &[u8]) {
    let target = root.join(assets::TARGET);
    let copied = fs::read(target.join("manifest.json")).unwrap();
    assert_eq!(copied, original);
    let manifest: assets::Manifest = serde_json::from_slice(&copied).unwrap();
    assert_eq!(manifest.git_sha, assets::head(root).unwrap());
    for (path, checksum) in manifest
        .sources
        .iter()
        .map(|source| (root.join(&source.path), &source.sha256))
        .chain(
            manifest
                .outputs
                .iter()
                .map(|output| (target.join(&output.path), &output.sha256)),
        )
    {
        assert_eq!(&assets::checksum(&path).unwrap(), checksum);
    }
}

#[given("the canonical ScreenWake artwork masters")]
fn sources(world: &mut Artwork) {
    let generated = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/asset-scenarios");
    fs::create_dir_all(&generated).unwrap();
    world.fixture = Some(tempfile::tempdir_in(generated.canonicalize().unwrap()).unwrap());
    let directory = world.root().join("code/macos/ScreenWake/Resources/Artwork");
    fs::create_dir_all(&directory).unwrap();
    for (name, bytes) in [
        (
            "app.svg",
            include_bytes!("../../../macos/ScreenWake/Resources/Artwork/app.svg").as_slice(),
        ),
        (
            "menu-off.svg",
            include_bytes!("../../../macos/ScreenWake/Resources/Artwork/menu-off.svg").as_slice(),
        ),
        (
            "menu-on.svg",
            include_bytes!("../../../macos/ScreenWake/Resources/Artwork/menu-on.svg").as_slice(),
        ),
        (
            "menu-error.svg",
            include_bytes!("../../../macos/ScreenWake/Resources/Artwork/menu-error.svg").as_slice(),
        ),
    ] {
        fs::write(directory.join(name), bytes).unwrap();
    }
    if let Some(complete) = COMPLETE_FIXTURE.get() {
        copy_directory(&complete.join(".git"), &world.root().join(".git"));
        assert_eq!(
            assets::head(world.root()).unwrap(),
            assets::head(complete).unwrap()
        );
    } else {
        git(world.root(), &["init", "--quiet"]);
        commit(world.root());
    }
}

#[when("the native asset exporter runs")]
async fn export(world: &mut Artwork) {
    let root = world.root().to_path_buf();
    let (sender, receiver) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let result = assets::head(&root)
            .and_then(|sha| assets::export(&root, &sha))
            .map_err(|error| error.to_string());
        sender
            .send(result)
            .expect("scenario still owns its exporter result");
    });
    world.result = Some(
        receiver
            .await
            .expect("export worker completed without losing its result"),
    );
}

#[given("a complete current asset export")]
async fn existing(world: &mut Artwork) {
    let root = world.root().to_path_buf();
    let (sender, receiver) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let complete = COMPLETE_FIXTURE
            .get()
            .expect("the shared source fixture exists");
        let original = COMPLETE_EXPORT
            .get_or_init(|| {
                assets::head(complete)
                    .and_then(|sha| assets::export(complete, &sha))
                    .and_then(|_| {
                        Ok(fs::read(
                            complete.join(assets::TARGET).join("manifest.json"),
                        )?)
                    })
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .expect("the production exporter must create the shared complete fixture");
        let target = root.join(assets::TARGET);
        copy_directory(&complete.join(assets::TARGET), &target);
        assert_export_identity(&root, original);
        sender
            .send(original.clone())
            .expect("scenario still owns its isolated fixture");
    });
    world.previous_manifest = Some(
        receiver
            .await
            .expect("shared fixture preparation completed"),
    );
}

#[given(expr = "the {string} master defect")]
fn defect(world: &mut Artwork, kind: String) {
    let path = world
        .root()
        .join("code/macos/ScreenWake/Resources/Artwork/menu-on.svg");
    match kind.as_str() {
        "missing" => fs::remove_file(path).unwrap(),
        "malformed" => fs::write(path, "<svg").unwrap(),
        "external" => {
            let xml = fs::read_to_string(&path).unwrap().replace(
                "</svg>",
                r#"<image href="https://invalid.example/never-fetch.png"/></svg>"#,
            );
            fs::write(path, xml).unwrap();
        }
        "dimensions" => {
            let xml = fs::read_to_string(&path)
                .unwrap()
                .replace("width=\"24\"", "width=\"25\"");
            fs::write(path, xml).unwrap();
        }
        other => panic!("unknown fixture defect: {other}"),
    }
}

#[then("it produces the complete native asset set and provenance manifest")]
fn exported(world: &mut Artwork) {
    let manifest = world
        .result
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("valid source must export successfully");
    assert_eq!(manifest.outputs.len(), 23);
    assert_eq!(manifest.sources.len(), 4);
    assert_eq!(manifest.git_sha, assets::head(world.root()).unwrap());
    assets::verify(world.root(), &manifest.git_sha)
        .expect("fresh export must verify at the actual consumer boundary");
    for (stem, logical, suffix, size) in [
        ("menu", 24, "", 24),
        ("menu", 24, "@2x", 48),
        ("preview", 96, "", 96),
        ("preview", 96, "@2x", 192),
    ] {
        let mut images = Vec::new();
        for state in ["off", "on", "error"] {
            let image = resvg::tiny_skia::Pixmap::load_png(
                world.target().join(format!("{stem}-{state}{suffix}.png")),
            )
            .unwrap();
            assert_eq!((image.width(), image.height()), (size, size));
            let record = manifest
                .outputs
                .iter()
                .find(|record| record.path == format!("{stem}-{state}{suffix}.png"))
                .unwrap();
            assert_eq!(record.representations[0].logical_size, logical);
            assert!(image.pixels().iter().any(|pixel| pixel.alpha() == 0));
            assert!(image.pixels().iter().any(|pixel| pixel.alpha() != 0));
            assert!(
                image
                    .pixels()
                    .iter()
                    .all(|pixel| pixel.red() == 0 && pixel.green() == 0 && pixel.blue() == 0)
            );
            images.push(image);
        }

        assert_ne!(
            images[0].data(),
            images[1].data(),
            "sleepy and awake templates must remain distinguishable"
        );
        assert_ne!(
            images[0].data(),
            images[2].data(),
            "Error is not asleep Off"
        );
        assert_ne!(images[1].data(), images[2].data(), "Error is not awake On");
    }
    let family = icns::IconFamily::read(std::io::BufReader::new(
        fs::File::open(world.target().join("app.icns")).unwrap(),
    ))
    .unwrap();
    let mut sizes: Vec<_> = family
        .available_icons()
        .into_iter()
        .map(|kind| {
            let icon = family.get_icon_with_type(kind).unwrap();
            assert_eq!(icon.width(), icon.height());
            icon.width()
        })
        .collect();
    sizes.sort();
    assert_eq!(sizes, [16, 32, 32, 64, 128, 256, 256, 512, 512, 1024]);
    for (logical, scale) in [
        (16, 1),
        (16, 2),
        (32, 1),
        (32, 2),
        (128, 1),
        (128, 2),
        (256, 1),
        (256, 2),
        (512, 1),
        (512, 2),
    ] {
        let path = world.target().join(format!(
            "app.iconset/icon_{logical}x{logical}{}.png",
            if scale == 2 { "@2x" } else { "" }
        ));
        let image = resvg::tiny_skia::Pixmap::load_png(path).unwrap();
        assert_eq!(
            (image.width(), image.height()),
            (logical * scale, logical * scale)
        );
    }
}

fn raster(world: &Artwork, state: &str, size: u32) -> resvg::tiny_skia::Pixmap {
    world
        .result
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("actual export");
    let (stem, suffix) = match size {
        24 => ("menu", ""),
        48 => ("menu", "@2x"),
        96 => ("preview", ""),
        192 => ("preview", "@2x"),
        _ => panic!("unsupported native density"),
    };
    resvg::tiny_skia::Pixmap::load_png(world.target().join(format!("{stem}-{state}{suffix}.png")))
        .unwrap()
}

fn effective_stroke(node: roxmltree::Node<'_, '_>) -> f64 {
    node.ancestors()
        .find_map(|node| node.attribute("stroke-width"))
        .expect("the rendered outline has an explicit or inherited stroke")
        .parse()
        .unwrap()
}

#[then(
    "all densities retain smaller paired left ears and equally weighted cabinet and screen borders"
)]
fn compact_ears_and_borders(world: &mut Artwork) {
    let mut failures = Vec::new();
    for state in ["off", "on", "error"] {
        let source = fs::read_to_string(world.root().join(format!(
            "code/macos/ScreenWake/Resources/Artwork/menu-{state}.svg"
        )))
        .unwrap();
        let document = roxmltree::Document::parse(&source).unwrap();
        let outline = |id| {
            document
                .descendants()
                .find(|node| node.attribute("id") == Some(id))
                .unwrap()
        };
        let cabinet = effective_stroke(outline("cabinet"));
        let screen = effective_stroke(outline("inset-screen"));
        if (cabinet - screen).abs() > f64::EPSILON {
            failures.push(format!(
                "{state}: cabinet stroke {cabinet} != screen {screen}"
            ));
        }
        assert_eq!(effective_stroke(outline("left-rabbit-ears")), 1.4);
        for size in [24, 48, 96, 192] {
            let image = raster(world, state, size);
            let scale = size / 24;
            let ink = |left, right, top, bottom| {
                (top * scale..bottom * scale)
                    .flat_map(|y| (left * scale..right * scale).map(move |x| (x, y)))
                    .filter(|&(x, y)| image.pixel(x, y).unwrap().alpha() > 64)
                    .count()
            };
            if ink(0, 10, 0, 2) != 0 {
                failures.push(format!(
                    "{state}/{size}: ears still reach the old tall silhouette"
                ));
            }
            assert!(
                ink(2, 6, 3, 6) > 0 && ink(6, 10, 3, 6) > 0,
                "{state}/{size}: both shortened left strokes must remain visible"
            );
            let column_ink = |top, bottom| -> u32 {
                (top * scale..bottom * scale)
                    .map(|y| u32::from(image.pixel(13 * scale, y).unwrap().alpha()))
                    .sum()
            };
            let outer = column_ink(6, 9);
            let inner = column_ink(9, 11);
            if outer.abs_diff(inner) > 2 {
                failures.push(format!(
                    "{state}/{size}: rendered border ink {outer} != {inner}"
                ));
            }
            assert!(outer > 0 && inner > 0);
            if image.pixel(13 * scale, 8 * scale).unwrap().alpha() != 0 {
                failures.push(format!(
                    "{state}/{size}: cabinet and inset need transparent separation"
                ));
            }
            println!("ARTWORK_BORDER {state}/{size} outer_alpha={outer} inner_alpha={inner}");
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[then(
    "the awake face has larger contained eyeballs with pupils and one continuous horizontal mouth"
)]
fn awake_eyes_and_mouth(world: &mut Artwork) {
    let source = fs::read_to_string(
        world
            .root()
            .join("code/macos/ScreenWake/Resources/Artwork/menu-on.svg"),
    )
    .unwrap();
    let document = roxmltree::Document::parse(&source).unwrap();
    let mut failures = Vec::new();
    let eyes: Vec<_> = document
        .descendants()
        .filter(|node| node.has_tag_name("ellipse"))
        .collect();
    assert_eq!(eyes.len(), 2);
    for eye in eyes {
        let number = |name| eye.attribute(name).unwrap().parse::<f64>().unwrap();
        let (cx, cy, rx, ry) = (number("cx"), number("cy"), number("rx"), number("ry"));
        let half_stroke = effective_stroke(eye) / 2.0;
        if rx < 2.3 || ry <= 2.7 {
            failures.push(format!(
                "awake eyeball at {cx}: still too small ({rx}, {ry})"
            ));
        }
        assert!(
            cx - rx - half_stroke > 6.0
                && cx + rx + half_stroke < 20.5
                && cy - ry - half_stroke > 10.0
                && cy + ry + half_stroke < 18.0,
            "actual stroked eye must stay inside the inset and above the mouth"
        );
    }
    for size in [24, 48, 96, 192] {
        let image = raster(world, "on", size);
        let scale = size / 24;
        let at = |x: f64, y: f64| {
            image
                .pixel(
                    (x * f64::from(scale)).floor() as u32,
                    (y * f64::from(scale)).floor() as u32,
                )
                .unwrap()
                .alpha()
        };
        for x in 10 * scale..17 * scale {
            if image
                .pixel(x, (18.5 * f64::from(scale)) as u32)
                .unwrap()
                .alpha()
                < 64
            {
                failures.push(format!(
                    "on/{size}: mouth is not one continuous horizontal line"
                ));
                break;
            }
        }
        for y in [17.5, 19.5] {
            if (10 * scale..17 * scale).any(|x| {
                image
                    .pixel(x, (y * f64::from(scale)) as u32)
                    .unwrap()
                    .alpha()
                    != 0
            }) {
                failures.push(format!(
                    "on/{size}: boxed/divided mouth leaves ink at y={y}"
                ));
            }
        }
        for cx in [9.5, 16.5] {
            assert!(at(cx, 13.9) > 64, "on/{size}: actual pupil remains visible");
            let runs = ((cx - 3.0) * f64::from(scale)).floor() as u32
                ..((cx + 3.0) * f64::from(scale)).ceil() as u32;
            let runs = runs
                .map(|x| {
                    image
                        .pixel(x, (13.9 * f64::from(scale)) as u32)
                        .unwrap()
                        .alpha()
                        > 64
                })
                .fold((0, false), |(runs, previous), ink| {
                    (runs + usize::from(ink && !previous), ink)
                })
                .0;
            if runs != 3 {
                failures.push(format!(
                    "on/{size}: outline/pupil/outline must remain distinct at {cx}, got {runs} runs"
                ));
            }
        }
        println!("ARTWORK_AWAKE size={size} mouth_y=18.5 pupil_y=13.9");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[then("the three ascending sleep marks leave a full logical pixel above the cabinet")]
fn sleep_clearance(world: &mut Artwork) {
    for size in [24, 48, 96, 192] {
        let image = raster(world, "off", size);
        let scale = size / 24;
        let bottom = (0..7 * scale)
            .filter(|&y| (12 * scale..15 * scale).any(|x| image.pixel(x, y).unwrap().alpha() != 0))
            .max()
            .expect("the smallest z visibly renders");
        let cabinet_top = (7 * scale..9 * scale)
            .find(|&y| image.pixel(13 * scale, y).unwrap().alpha() != 0)
            .expect("the cabinet visibly renders");
        let clear_rows = cabinet_top.saturating_sub(bottom + 1);
        assert!(
            clear_rows >= scale,
            "off/{size}: smallest z needs {scale} fully transparent rows, got {clear_rows}"
        );
        let centers: Vec<_> = [(12, 15), (16, 19), (20, 24)]
            .into_iter()
            .map(|(left, right)| {
                (0..7 * scale)
                    .flat_map(|y| (left * scale..right * scale).map(move |x| (x, y)))
                    .fold((0.0, 0.0), |(weighted_y, weight), (x, y)| {
                        let alpha = f64::from(image.pixel(x, y).unwrap().alpha());
                        (weighted_y + f64::from(y) * alpha, weight + alpha)
                    })
            })
            .map(|(weighted_y, weight)| {
                assert!(weight > 0.0, "all three sleep marks render");
                weighted_y / weight / f64::from(scale)
            })
            .collect();
        assert!(
            centers.windows(2).all(|pair| pair[0] - pair[1] > 0.5),
            "sleep marks ascend right"
        );
        if size >= 48 {
            assert!(
                (0..size).all(|x| image.pixel(x, 0).unwrap().alpha() == 0),
                "resolved stroke caps are not clipped at the top"
            );
        }
        println!(
            "ARTWORK_SLEEP size={size} clear_rows={clear_rows} centers={centers:?} logical_gap={}",
            f64::from(clear_rows) / f64::from(scale)
        );
    }
}

#[then(
    "rendered TV faces have inset screens and left ears with sleep marks only at the Off upper right"
)]
fn tv_faces(world: &mut Artwork) {
    world
        .result
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("canonical sources must actually render");
    for state in ["off", "on", "error"] {
        let source = fs::read_to_string(world.root().join(format!(
            "code/macos/ScreenWake/Resources/Artwork/menu-{state}.svg"
        )))
        .unwrap();
        let document = roxmltree::Document::parse(&source).unwrap();
        let rectangles: Vec<[f64; 4]> = ["cabinet", "inset-screen"]
            .into_iter()
            .map(|name| {
                let node = document
                    .descendants()
                    .find(|node| node.attribute("id") == Some(name))
                    .expect("both the outer cabinet and inset screen must exist");
                ["x", "y", "width", "height"]
                    .map(|key| node.attribute(key).unwrap().parse().unwrap())
            })
            .collect();
        let [x, y, width, height] = rectangles[0];
        let [sx, sy, sw, sh] = rectangles[1];
        assert!(
            width > height * 1.4 && sw > sh * 1.4,
            "cabinet and screen read as landscape television, not a rounded square"
        );
        assert!(
            sx > x && sy > y && sx + sw < x + width && sy + sh < y + height,
            "the screen outline must sit distinctly inside the cabinet"
        );
        for (stem, suffix, size) in [
            ("menu", "", 24),
            ("menu", "@2x", 48),
            ("preview", "", 96),
            ("preview", "@2x", 192),
        ] {
            let image = resvg::tiny_skia::Pixmap::load_png(
                world.target().join(format!("{stem}-{state}{suffix}.png")),
            )
            .unwrap();
            let alpha = |x: f64, y: f64| {
                let x = (x * f64::from(size) / 24.0).floor() as u32;
                let y = (y * f64::from(size) / 24.0).floor() as u32;
                image.pixel(x, y).unwrap().alpha()
            };
            assert!(
                alpha(sx, 18.0) > 64,
                "{state} at {size}px must visibly render the inset screen independently of its outer cabinet"
            );
            let ink = |left: u32, right: u32| -> usize {
                (0..size * 6 / 24)
                    .flat_map(|y| (left * size / 24..right * size / 24).map(move |x| (x, y)))
                    .filter(|&(x, y)| image.pixel(x, y).unwrap().alpha() > 64)
                    .count()
            };
            assert!(ink(0, 11) > 0, "both ears belong at the upper left");
            assert!(
                alpha(3.5, 4.0) > 64 && alpha(8.0, 3.5) > 64,
                "both distinct shortened left rabbit-ear strokes must actually render"
            );
            assert_eq!(
                ink(12, 24) > 0,
                state == "off",
                "only sleeping Off has upper-right Zzz"
            );
            if size >= 96 {
                match state {
                    "off" => {
                        assert!(
                            alpha(9.0, 14.25) > 64 && alpha(9.0, 11.5) == 0,
                            "Off eyes are closed, not wide-open or crossed"
                        );
                        assert!(
                            alpha(13.0, 18.75) > 64 && alpha(13.0, 17.0) == 0,
                            "the sleeping mouth curves down to a smile"
                        );
                    }
                    "on" => {
                        assert!(alpha(9.5, 10.8) > 64, "On eyes open very wide");
                        for x in [10.0, 12.0, 14.0, 16.0] {
                            assert!(alpha(x, 18.5) > 64, "On has one continuous mouth line");
                        }
                        assert_eq!(alpha(13.0, 17.5), 0, "mouth has no upper box edge");
                        assert_eq!(alpha(13.0, 19.5), 0, "mouth has no lower box edge");
                    }
                    "error" => {
                        for x in [7.5, 10.5, 15.5, 18.5] {
                            for y in [12.0, 15.0] {
                                assert!(alpha(x, y) > 64, "Error eyes visibly cross");
                            }
                        }
                        assert!(
                            alpha(13.0, 17.75) > 64 && alpha(13.0, 19.5) == 0,
                            "the attention mouth curves up to a frown"
                        );
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[then("all native densities distinguish two left selectors from the inset screen border")]
fn selector_dots(world: &mut Artwork) {
    world
        .result
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("actual export");
    for state in ["off", "on", "error"] {
        for (stem, suffix, size) in [
            ("menu", "", 24),
            ("menu", "@2x", 48),
            ("preview", "", 96),
            ("preview", "@2x", 192),
        ] {
            let image = resvg::tiny_skia::Pixmap::load_png(
                world.target().join(format!("{stem}-{state}{suffix}.png")),
            )
            .unwrap();
            let alpha = |x: f64, y: f64| {
                image
                    .pixel(
                        (x * f64::from(size) / 24.0).floor() as u32,
                        (y * f64::from(size) / 24.0).floor() as u32,
                    )
                    .unwrap()
                    .alpha()
            };
            assert!(
                alpha(3.0, 12.5) > 64 && alpha(3.0, 17.5) > 64,
                "{state}/{size}: both selector dots must visibly render"
            );
            assert_eq!(
                alpha(3.0, 15.0),
                0,
                "{state}/{size}: separate dots cannot be the old continuous screen border"
            );
            assert!(
                alpha(5.0, 15.0) > 64,
                "{state}/{size}: moved screen border remains visible"
            );
            assert_eq!(
                alpha(4.0, 15.0),
                0,
                "{state}/{size}: controls and screen retain clear negative space"
            );
            let selector_runs = (10 * size / 24..20 * size / 24)
                .map(|y| image.pixel(3 * size / 24, y).unwrap().alpha() > 64)
                .fold((0, false), |(runs, previous), ink| {
                    (runs + usize::from(ink && !previous), ink)
                })
                .0;
            assert_eq!(
                selector_runs, 2,
                "{state}/{size}: exactly two isolated controls"
            );
            let region_ink = |left: u32, right: u32, top: u32, bottom: u32| {
                (top * size / 24..bottom * size / 24)
                    .flat_map(|y| (left * size / 24..right * size / 24).map(move |x| (x, y)))
                    .any(|(x, y)| image.pixel(x, y).unwrap().alpha() > 64)
            };
            assert!(
                region_ink(7, 11, 11, 16)
                    && region_ink(15, 19, 11, 16)
                    && region_ink(9, 17, 17, 20),
                "{state}/{size}: both eyes and the mouth remain legible inside the inset"
            );
        }
    }
}

#[then(expr = "export fails with {string} and no published manifest")]
fn rejected(world: &mut Artwork, message: String) {
    let error = world.result.as_ref().unwrap().as_ref().unwrap_err();
    assert!(error.contains(&message), "{error}");
    assert!(!world.target().join("manifest.json").exists());
}

#[when("a valid source changes after export")]
fn change_source(world: &mut Artwork) {
    let path = world
        .root()
        .join("code/macos/ScreenWake/Resources/Artwork/app.svg");
    let xml = fs::read_to_string(&path).unwrap().replace(
        "AI2ScreenWake sleepy television",
        "AI2ScreenWake sleeping television",
    );
    fs::write(path, xml).unwrap();
}

#[then("current verification rejects stale sources")]
fn stale_source(world: &mut Artwork) {
    let error = assets::verify(world.root(), &assets::head(world.root()).unwrap())
        .unwrap_err()
        .to_string();
    assert!(error.contains("stale or mixed source"), "{error}");
}

#[when("the fixture advances to a new commit")]
fn advance(world: &mut Artwork) {
    commit(world.root());
}

#[then("current verification rejects stale commit identity")]
fn stale_head(world: &mut Artwork) {
    let error = assets::verify(world.root(), &assets::head(world.root()).unwrap())
        .unwrap_err()
        .to_string();
    assert!(error.contains("commit/target/renderer"), "{error}");
}

#[when("an unexpected output appears")]
fn extra(world: &mut Artwork) {
    fs::write(world.target().join("unrecognized.txt"), b"preserve me").unwrap();
}

#[then("verification rejects the extra output and regeneration preserves it")]
fn preserve_extra(world: &mut Artwork) {
    let sha = assets::head(world.root()).unwrap();
    assert!(assets::verify(world.root(), &sha).is_err());
    assert!(assets::export(world.root(), &sha).is_err());
    assert_eq!(
        fs::read(world.target().join("unrecognized.txt")).unwrap(),
        b"preserve me"
    );
    assert_eq!(
        fs::read(world.target().join("manifest.json")).unwrap(),
        *world.previous_manifest.as_ref().unwrap()
    );
}

#[then("export fails without changing the previous manifest")]
fn preserve_previous(world: &mut Artwork) {
    assert!(world.result.as_ref().unwrap().is_err());
    assert_eq!(
        fs::read(world.target().join("manifest.json")).unwrap(),
        *world.previous_manifest.as_ref().unwrap()
    );
    assert!(assets::verify(world.root(), &assets::head(world.root()).unwrap()).is_err());
}

fn main() {
    let started = Instant::now();
    let mut complete = Artwork::default();
    sources(&mut complete);
    COMPLETE_FIXTURE.set(complete.root().to_path_buf()).unwrap();
    futures::executor::block_on(
        Artwork::cucumber()
            .max_concurrent_scenarios(3)
            .fail_on_skipped()
            .run_and_exit("tests/features/assets.feature"),
    );
    if let Some(original_manifest) = COMPLETE_EXPORT.get() {
        assert_export_identity(complete.root(), original_manifest.as_ref().unwrap());
    }
    drop(complete);
    assert!(
        started.elapsed().as_secs_f64() < 10.0,
        "focused asset scenarios exceeded 10 seconds"
    );
    println!(
        "ASSET_SCENARIOS_OK workers=3 elapsed_seconds={:.6}",
        started.elapsed().as_secs_f64()
    );
}
// end assets.rs
