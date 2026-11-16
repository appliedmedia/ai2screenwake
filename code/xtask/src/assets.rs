// assets.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Fixed, font-free SVG masters become verified native assets without external renderers.

use icns::{IconFamily, IconType, Image, PixelFormat};
use resvg::{tiny_skia, usvg};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    error::Error,
    fs::{self, File},
    io::{BufReader, Cursor},
    path::{Path, PathBuf},
    process::Command,
};

/// Shared tooling errors always propagate to the measured command boundary.
pub(crate) type Result<T> = std::result::Result<T, Box<dyn Error>>;
/// The sole generated asset directory consumed by native packaging.
pub(crate) const TARGET: &str = "target/screenwake/assets";
const INPUT: &str = "code/macos/ScreenWake/Resources/Artwork";
const MASTERS: [(&str, u32); 3] = [("app.svg", 1024), ("menu-off.svg", 24), ("menu-on.svg", 24)];
const APP_SIZES: [(u32, u32, IconType); 10] = [
    (16, 1, IconType::RGBA32_16x16),
    (16, 2, IconType::RGBA32_16x16_2x),
    (32, 1, IconType::RGBA32_32x32),
    (32, 2, IconType::RGBA32_32x32_2x),
    (128, 1, IconType::RGBA32_128x128),
    (128, 2, IconType::RGBA32_128x128_2x),
    (256, 1, IconType::RGBA32_256x256),
    (256, 2, IconType::RGBA32_256x256_2x),
    (512, 1, IconType::RGBA32_512x512),
    (512, 2, IconType::RGBA32_512x512_2x),
];

/// Hash actual bytes, not a path or modification timestamp.
pub(crate) fn checksum(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

/// Resolve the current commit and reject unsuccessful Git observations.
pub(crate) fn head(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse HEAD exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// A source's exact byte identity and required logical coordinate system.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    pub path: String,
    pub sha256: String,
    pub view_box: [u32; 4],
}

/// Pixel dimensions are distinct from native logical dimensions and density.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Representation {
    pub width: u32,
    pub height: u32,
    pub logical_size: u32,
    pub scale: u32,
}

/// A PNG has one representation; the ICNS container has all ten.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifact {
    pub path: String,
    pub source: String,
    pub sha256: String,
    pub representations: Vec<Representation>,
}

/// Rendering deliberately excludes fonts, raster imports, and external resources.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Renderer {
    svg: String,
    png: String,
    icns: String,
    dpi: u32,
    background: String,
    color_space: String,
    system_fonts: bool,
    external_resources: bool,
}

impl Renderer {
    /// Pin settings as well as versions so verification rejects mixed exports.
    fn canonical() -> Self {
        Self {
            svg: "resvg 0.45.1".into(),
            png: "tiny-skia 0.11.4".into(),
            icns: "icns 0.4.0 (PNG only)".into(),
            dpi: 96,
            background: "transparent".into(),
            color_space: "sRGB".into(),
            system_fonts: false,
            external_resources: false,
        }
    }
}

/// A complete export is admissible only with current inputs, head, and outputs.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    schema_version: u32,
    pub git_sha: String,
    target: String,
    platform: String,
    renderer: Renderer,
    pub sources: Vec<Source>,
    pub outputs: Vec<Artifact>,
}

/// Canonical PNG output metadata, never read from an untrusted manifest.
struct Png {
    path: String,
    source: usize,
    representation: Representation,
    icon_type: Option<IconType>,
}

/// Enumerate the fixed iconset and menu representations in stable order.
fn pngs() -> Vec<Png> {
    let mut images = Vec::new();
    for (logical_size, scale, icon_type) in APP_SIZES {
        images.push(Png {
            path: format!(
                "app.iconset/icon_{logical_size}x{logical_size}{}.png",
                if scale == 2 { "@2x" } else { "" }
            ),
            source: 0,
            representation: Representation {
                width: logical_size * scale,
                height: logical_size * scale,
                logical_size,
                scale,
            },
            icon_type: Some(icon_type),
        });
    }
    for (source, stem) in [(1, "menu-off"), (2, "menu-on")] {
        for scale in [1, 2] {
            images.push(Png {
                path: format!("{stem}{}.png", if scale == 2 { "@2x" } else { "" }),
                source,
                representation: Representation {
                    width: 24 * scale,
                    height: 24 * scale,
                    logical_size: 24,
                    scale,
                },
                icon_type: None,
            });
        }
    }
    images
}

/// Refuse existing symlinks before reading or creating any canonical path.
fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = root.join(relative);
    if !path.starts_with(root)
        || Path::new(relative)
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(format!("unsafe asset path: {relative}").into());
    }
    for parent in path.ancestors().take_while(|parent| *parent != root) {
        match fs::symlink_metadata(parent) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(
                    format!("asset symlink is not admissible: {}", parent.display()).into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

/// An immutable source snapshot is used for every size of its image.
struct Master {
    identity: Source,
    tree: usvg::Tree,
}

/// Reject XML features that could load files, network resources, or fonts.
fn parse_svg(bytes: &[u8], size: u32) -> Result<usvg::Tree> {
    let xml = roxmltree::Document::parse(std::str::from_utf8(bytes)?)?;
    let root = xml.root_element();
    if root.tag_name().name() != "svg"
        || root.tag_name().namespace() != Some("http://www.w3.org/2000/svg")
    {
        return Err("artwork must have an SVG root".into());
    }
    let view_box = root
        .attribute("viewBox")
        .ok_or("missing SVG viewBox")?
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if root.attribute("width") != Some(size.to_string().as_str())
        || root.attribute("height") != Some(size.to_string().as_str())
        || view_box != [0.0, 0.0, f64::from(size), f64::from(size)]
    {
        return Err("invalid artwork dimensions or viewBox".into());
    }
    let tags = [
        "svg",
        "title",
        "desc",
        "defs",
        "linearGradient",
        "stop",
        "g",
        "path",
        "rect",
        "circle",
        "ellipse",
    ];
    let attributes = [
        "width",
        "height",
        "viewBox",
        "role",
        "aria-labelledby",
        "id",
        "x1",
        "x2",
        "y1",
        "y2",
        "offset",
        "stop-color",
        "cx",
        "cy",
        "r",
        "rx",
        "ry",
        "fill",
        "opacity",
        "d",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "x",
        "y",
    ];
    for node in xml.descendants().filter(roxmltree::Node::is_element) {
        if !tags.contains(&node.tag_name().name())
            || node.tag_name().namespace() != Some("http://www.w3.org/2000/svg")
        {
            return Err(format!("unsafe SVG element: {}", node.tag_name().name()).into());
        }
        for attribute in node.attributes() {
            if attribute.namespace().is_some() || !attributes.contains(&attribute.name()) {
                return Err(format!("unsafe SVG attribute: {}", attribute.name()).into());
            }
            if attribute.value().contains("url(") {
                let id = attribute
                    .value()
                    .strip_prefix("url(#")
                    .and_then(|value| value.strip_suffix(')'))
                    .ok_or("external SVG resources are forbidden")?;
                if !xml.descendants().any(|candidate| {
                    candidate.attribute("id") == Some(id)
                        && candidate.tag_name().name() == "linearGradient"
                }) {
                    return Err("SVG references a missing or unsupported paint resource".into());
                }
            }
        }
    }
    let options = usvg::Options {
        dpi: 96.0,
        ..usvg::Options::default()
    };
    Ok(usvg::Tree::from_data(bytes, &options)?)
}

/// Read and validate all three masters before creating any export.
fn masters(root: &Path) -> Result<Vec<Master>> {
    MASTERS
        .iter()
        .map(|(name, size)| {
            let relative = format!("{INPUT}/{name}");
            let path = safe_path(root, &relative)?;
            let bytes = fs::read(&path)
                .map_err(|error| format!("cannot read master {relative}: {error}"))?;
            Ok(Master {
                identity: Source {
                    path: relative,
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    view_box: [0, 0, *size, *size],
                },
                tree: parse_svg(&bytes, *size)
                    .map_err(|error| format!("invalid master {name}: {error}"))?,
            })
        })
        .collect()
}

/// Require actual visible artwork, transparent margins, and black-only menu pixels.
fn validate_pixels(
    pixmap: &tiny_skia::Pixmap,
    representation: &Representation,
    menu: bool,
) -> Result<()> {
    if pixmap.width() != representation.width
        || pixmap.height() != representation.height
        || !pixmap.pixels().iter().any(|pixel| pixel.alpha() == 0)
        || !pixmap.pixels().iter().any(|pixel| pixel.alpha() > 0)
    {
        return Err("asset has invalid dimensions, empty artwork, or no transparency".into());
    }
    if menu
        && pixmap
            .pixels()
            .iter()
            .any(|pixel| pixel.red() != 0 || pixel.green() != 0 || pixel.blue() != 0)
    {
        return Err("menu template artwork must be black with alpha".into());
    }
    Ok(())
}

/// Enumerate generated files without following symlinks or extra directories.
fn output_files(directory: &Path) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "invalid output filename")?;
        let kind = entry.file_type()?;
        if kind.is_file() {
            names.insert(name);
        } else if kind.is_dir() && name == "app.iconset" {
            for icon in fs::read_dir(entry.path())? {
                let icon = icon?;
                if !icon.file_type()?.is_file() {
                    return Err("unexpected directory or symlink in generated iconset".into());
                }
                names.insert(format!(
                    "app.iconset/{}",
                    icon.file_name()
                        .into_string()
                        .map_err(|_| "invalid icon filename")?
                ));
            }
        } else {
            return Err(format!("unexpected output directory or symlink: {name}").into());
        }
    }
    Ok(names)
}

/// Validate provenance, exact inventory, decoded PNGs, and every ICNS representation.
fn verify_directory(root: &Path, directory: &Path, sha: &str) -> Result<Manifest> {
    let pngs = pngs();
    let mut expected: BTreeSet<_> = pngs.iter().map(|image| image.path.clone()).collect();
    expected.extend(["app.icns".to_owned(), "manifest.json".to_owned()]);
    if output_files(directory)? != expected {
        return Err("asset output inventory has missing, extra, or duplicate entries".into());
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    if manifest.schema_version != 1
        || manifest.git_sha != sha
        || head(root)? != sha
        || manifest.target != TARGET
        || manifest.platform != "macos"
        || manifest.renderer != Renderer::canonical()
    {
        return Err(
            "asset manifest has stale or incompatible commit/target/renderer identity".into(),
        );
    }
    let sources: Vec<_> = masters(root)?
        .into_iter()
        .map(|master| master.identity)
        .collect();
    if sources != manifest.sources {
        return Err("asset manifest has stale or mixed source identity".into());
    }
    if manifest.outputs.len() != pngs.len() + 1 {
        return Err("asset output inventory has missing, extra, or duplicate entries".into());
    }
    for (record, image) in manifest.outputs.iter().zip(&pngs) {
        if record.path != image.path
            || record.source != sources[image.source].path
            || record.representations != [image.representation.clone()]
            || checksum(&directory.join(&record.path))? != record.sha256
        {
            return Err(format!("asset metadata/checksum mismatch: {}", image.path).into());
        }
        validate_pixels(
            &tiny_skia::Pixmap::load_png(directory.join(&image.path))?,
            &image.representation,
            image.source != 0,
        )?;
    }
    let container = manifest.outputs.last().ok_or("missing ICNS record")?;
    if container.path != "app.icns"
        || container.source != sources[0].path
        || container.sha256 != checksum(&directory.join("app.icns"))?
        || container.representations
            != pngs
                .iter()
                .filter(|image| image.source == 0)
                .map(|image| image.representation.clone())
                .collect::<Vec<_>>()
    {
        return Err("ICNS metadata/checksum mismatch".into());
    }
    let family = IconFamily::read(BufReader::new(File::open(directory.join("app.icns"))?))?;
    if family.elements.len() != APP_SIZES.len() || family.available_icons().len() != APP_SIZES.len()
    {
        return Err("ICNS has missing or extra representations".into());
    }
    for image in pngs.iter().filter(|image| image.source == 0) {
        let icon = family
            .get_icon_with_type(image.icon_type.ok_or("missing canonical ICNS type")?)?
            .convert_to(PixelFormat::RGBA);
        let png = Image::read_png(BufReader::new(File::open(directory.join(&image.path))?))?
            .convert_to(PixelFormat::RGBA);
        if icon.width() != image.representation.width
            || icon.height() != image.representation.height
            || icon.data() != png.data()
        {
            return Err("decoded ICNS does not match its PNG representation".into());
        }
    }
    Ok(manifest)
}

/// Fail closed on any stale, missing, malformed, or unexpected generated artifact.
pub(crate) fn verify(root: &Path, sha: &str) -> Result<Manifest> {
    let root = root.canonicalize()?;
    verify_directory(&root, &safe_path(&root, TARGET)?, sha)
}

/// Publish only a fully verified directory, retaining old outputs if publication fails.
fn publish(staging: &Path, destination: &Path) -> Result<()> {
    if !destination.exists() {
        fs::rename(staging, destination)?;
        return Ok(());
    }
    let expected: BTreeSet<_> = pngs()
        .into_iter()
        .map(|image| image.path)
        .chain(["app.icns".into(), "manifest.json".into()])
        .collect();
    if !output_files(destination)?.is_subset(&expected) {
        return Err("refusing to replace unrecognized files in the asset target".into());
    }
    let backup = tempfile::Builder::new()
        .prefix(".assets-previous-")
        .tempdir_in(destination.parent().ok_or("missing asset parent")?)?;
    let previous = backup.path().join("assets");
    fs::rename(destination, &previous)?;
    if let Err(error) = fs::rename(staging, destination) {
        if let Err(restore_error) = fs::rename(&previous, destination) {
            let preserved = backup.keep();
            return Err(format!("asset publish failed: {error}; restore failed: {restore_error}; previous assets preserved at {}", preserved.display()).into());
        }
        return Err(format!("asset publish failed; previous assets restored: {error}").into());
    }
    backup.close()?;
    Ok(())
}

/// Render from immutable source snapshots, verify, then replace the canonical output.
///
/// A failed input or encoding never publishes a partial success manifest.
pub(crate) fn export(root: &Path, sha: &str) -> Result<Manifest> {
    let root = root.canonicalize()?;
    if head(&root)? != sha {
        return Err("asset export commit changed before rendering".into());
    }
    let sources = masters(&root)?;
    let destination = safe_path(&root, TARGET)?;
    let parent = destination.parent().ok_or("missing asset output parent")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".assets-stage-")
        .tempdir_in(parent)?;
    let directory = staging.path().join("assets");
    fs::create_dir_all(directory.join("app.iconset"))?;
    let mut family = IconFamily::new();
    let mut outputs = Vec::new();
    for image in pngs() {
        let master = &sources[image.source];
        let size = image.representation.width;
        let mut pixmap =
            tiny_skia::Pixmap::new(size, size).ok_or("cannot allocate asset pixels")?;
        let scale = size as f32 / master.tree.size().width();
        resvg::render(
            &master.tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        validate_pixels(&pixmap, &image.representation, image.source != 0)?;
        let png = pixmap.encode_png()?;
        if let Some(icon_type) = image.icon_type {
            family.add_icon_with_type(&Image::read_png(Cursor::new(&png))?, icon_type)?;
        }
        let path = directory.join(&image.path);
        fs::write(&path, png)?;
        outputs.push(Artifact {
            path: image.path,
            source: master.identity.path.clone(),
            sha256: checksum(&path)?,
            representations: vec![image.representation],
        });
    }
    let mut icon = Vec::new();
    family.write(&mut icon)?;
    fs::write(directory.join("app.icns"), icon)?;
    outputs.push(Artifact {
        path: "app.icns".into(),
        source: sources[0].identity.path.clone(),
        sha256: checksum(&directory.join("app.icns"))?,
        representations: outputs
            .iter()
            .filter(|image| image.source == sources[0].identity.path)
            .flat_map(|image| image.representations.clone())
            .collect(),
    });
    let manifest = Manifest {
        schema_version: 1,
        git_sha: sha.to_owned(),
        target: TARGET.into(),
        platform: "macos".into(),
        renderer: Renderer::canonical(),
        sources: sources.into_iter().map(|master| master.identity).collect(),
        outputs,
    };
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    verify_directory(&root, &directory, sha)?;
    publish(&directory, &destination)?;
    verify(&root, sha)
}
// end assets.rs
