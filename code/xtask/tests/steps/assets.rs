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
                alpha(3.0, 18.0) > 64,
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
                alpha(2.0, 2.0) > 64 && alpha(9.0, 1.5) > 64,
                "both distinct left rabbit-ear strokes must actually render"
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
                            alpha(7.5, 14.25) > 64 && alpha(7.5, 11.5) == 0,
                            "Off eyes are closed, not wide-open or crossed"
                        );
                        assert!(
                            alpha(12.0, 18.75) > 64 && alpha(12.0, 17.0) == 0,
                            "the sleeping mouth curves down to a smile"
                        );
                    }
                    "on" => {
                        assert!(alpha(7.5, 10.7) > 64, "On eyes open very wide");
                        for tooth in [10.0, 12.0, 14.0] {
                            assert!(alpha(tooth, 18.5) > 64, "On has visible gritted teeth");
                        }
                        assert_eq!(alpha(11.0, 18.5), 0, "teeth retain transparent gaps");
                    }
                    "error" => {
                        for x in [6.0, 9.0, 15.0, 18.0] {
                            for y in [12.0, 15.0] {
                                assert!(alpha(x, y) > 64, "Error eyes visibly cross");
                            }
                        }
                        assert!(
                            alpha(12.0, 17.75) > 64 && alpha(12.0, 19.5) == 0,
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
