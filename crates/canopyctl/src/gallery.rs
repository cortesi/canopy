//! Screenshot galleries: gallery scripts run at each screen size, their
//! captures drawn as PNG images, and a static viewer beside them.
//!
//! A gallery script drives the app into the states it shows, and returns a
//! list of shots. A shot is a table with an `id`, an optional `title` and
//! `caption`, and a `capture` from `canopy.capture()`. One page of the
//! viewer holds the images of one shot ID at every size.
//!
//! A run of named scripts updates only their pages: the pages and failures of
//! the other scripts stay as the last run published them, while the font and
//! the scale stay the same.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use canopy::render::ScreenCapture;
use canopy_mcp::{ScreenSize, SuiteConfig, plan_suite};
use canopy_widgets::screenshot::{Screenshot, ScreenshotOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{config::GallerySettings, parse_screen, session::Session};

/// The viewer page. The manifest takes the place of [`PLACEHOLDER`].
const VIEWER: &str = include_str!("gallery.html");

/// The manifest slot of the viewer.
const PLACEHOLDER: &str = "/*GALLERY*/null";

/// The manifest file in the gallery directory.
const MANIFEST: &str = "gallery.json";

/// One shot that a gallery script returns.
#[derive(Deserialize)]
struct Shot {
    /// Page identity, which also names the image files.
    id: String,
    /// Page title. The ID serves without one.
    #[serde(default)]
    title: Option<String>,
    /// What the page shows.
    #[serde(default)]
    caption: Option<String>,
    /// The frame.
    capture: ScreenCapture,
}

/// The gallery manifest, which the viewer reads.
#[derive(Serialize, Deserialize)]
struct Manifest {
    /// Viewer title.
    title: String,
    /// Generation time, in seconds since the Unix epoch.
    generated: u64,
    /// Font size of the images, before the scale.
    font_size: f32,
    /// Image pixels for each CSS pixel, so that images show at their size.
    scale: f32,
    /// Screen sizes: the sizes of the run, then other sizes of kept pages.
    sizes: Vec<String>,
    /// Pages, in script and shot order.
    pages: Vec<Page>,
    /// Script runs that failed.
    failures: Vec<Failure>,
    /// Characters that the screenshot font lacks, in every image.
    missing_glyphs: Vec<char>,
}

/// One page: the images of one shot at each size.
#[derive(Serialize, Deserialize)]
struct Page {
    /// Shot identity.
    id: String,
    /// Title.
    title: String,
    /// What the page shows.
    caption: Option<String>,
    /// Script that captured the page, relative to the suite.
    script: String,
    /// Images, in size order.
    shots: Vec<Image>,
}

/// One image file.
#[derive(Serialize, Deserialize)]
struct Image {
    /// Screen size as `WIDTHxHEIGHT`.
    size: String,
    /// File name in the gallery directory.
    file: String,
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
    /// Characters that the screenshot font lacks, which show as boxes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    missing: Vec<char>,
}

/// One failure of a script.
#[derive(Serialize, Deserialize)]
struct Failure {
    /// Script, relative to the suite.
    script: String,
    /// Screen size as `WIDTHxHEIGHT`, when the failure has one.
    size: Option<String>,
    /// What failed.
    message: String,
}

/// The pages and failures of one run, as the scripts produce them.
struct Run {
    /// Draws the captures.
    shooter: Screenshot,
    /// Directory that receives the images.
    staging: PathBuf,
    /// Pages in shot order.
    pages: Vec<Page>,
    /// Page index of each shot ID.
    index: HashMap<String, usize>,
    /// Failures.
    failures: Vec<Failure>,
}

impl Run {
    /// Record a failure of a script.
    fn fail(&mut self, script: &str, size: &str, message: String) {
        println!("  {message}");
        self.failures.push(Failure {
            script: script.to_owned(),
            size: Some(size.to_owned()),
            message,
        });
    }

    /// Draw one shot of a script at a size, and file it under its page.
    fn add(&mut self, script: &str, size: &str, shot: &Shot) -> Result<(), String> {
        check_id(&shot.id)?;
        let at = *self.index.entry(shot.id.clone()).or_insert_with(|| {
            self.pages.push(Page {
                id: shot.id.clone(),
                title: shot.title.clone().unwrap_or_else(|| shot.id.clone()),
                caption: shot.caption.clone(),
                script: script.to_owned(),
                shots: Vec::new(),
            });
            self.pages.len() - 1
        });
        let page = &mut self.pages[at];
        if page.script != script || page.shots.iter().any(|image| image.size == size) {
            return Err(format!("shot ID `{}` repeats", shot.id));
        }
        let png = self
            .shooter
            .png(&shot.capture)
            .map_err(|error| format!("shot `{}`: {error}", shot.id))?;
        let file = format!("{}-{size}.png", shot.id);
        fs::write(self.staging.join(&file), png.data)
            .map_err(|error| format!("write {file}: {error}"))?;
        page.shots.push(Image {
            size: size.to_owned(),
            file,
            width: png.width,
            height: png.height,
            missing: png.missing,
        });
        Ok(())
    }
}

/// Run the gallery scripts at every size, and publish the images and the
/// viewer. Named scripts update only their pages. The gallery publishes even
/// when a run fails, and the failures show at the top of the viewer.
pub async fn run(
    session: &Session,
    settings: &GallerySettings,
    scripts: Vec<PathBuf>,
) -> Result<()> {
    let sizes = sizes(&settings.sizes)?;
    let partial = !scripts.is_empty();
    let mut suite = SuiteConfig::new(&settings.suite);
    suite.scripts = scripts;
    suite.timeout_ms = settings.timeout_ms;
    let plan = plan_suite(&suite)?;
    if plan.is_empty() {
        bail!("no gallery scripts in {}", settings.suite.display());
    }
    check_out(&settings.out)?;
    let staging = sibling(&settings.out, "staging")?;
    if staging.exists() {
        fs::remove_dir_all(&staging).with_context(|| format!("remove {}", staging.display()))?;
    }
    fs::create_dir_all(&staging).with_context(|| format!("create {}", staging.display()))?;

    let mut run = Run {
        shooter: Screenshot::new(ScreenshotOptions {
            font_size: settings.font_size,
            scale: settings.scale,
        })?,
        staging: staging.clone(),
        pages: Vec::new(),
        index: HashMap::new(),
        failures: Vec::new(),
    };
    let mut ran = BTreeSet::new();
    for script in plan {
        let name = script
            .path
            .strip_prefix(&settings.suite)
            .unwrap_or(&script.path)
            .display()
            .to_string();
        ran.insert(name.clone());
        for (label, screen) in &sizes {
            let mut request = script.request.clone();
            request.screen = Some(*screen);
            let started = Instant::now();
            let report = session.eval(request).await?;
            let elapsed = started.elapsed().as_millis();
            let shots = if report.success {
                shots(report.value).map_err(|error| format!("{error:#}"))
            } else {
                Err(report
                    .error
                    .map_or_else(|| "the script failed".to_owned(), |error| error.message))
            };
            let shots = match shots {
                Ok(shots) => shots,
                Err(message) => {
                    println!("FAIL gallery={name} size={label} ({elapsed}ms)");
                    run.fail(&name, label, message);
                    continue;
                }
            };
            println!(
                "PASS gallery={name} size={label} ({elapsed}ms) {} shot(s)",
                shots.len()
            );
            for shot in &shots {
                if let Err(message) = run.add(&name, label, shot) {
                    run.fail(&name, label, message);
                }
            }
        }
    }

    let failed = run.failures.len();
    let mut manifest = Manifest {
        title: settings.title.clone(),
        generated: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs()),
        font_size: settings.font_size,
        scale: settings.scale,
        sizes: sizes.into_iter().map(|(label, _)| label).collect(),
        pages: run.pages,
        failures: run.failures,
        missing_glyphs: Vec::new(),
    };
    if partial {
        keep_others(&settings.out, &staging, &ran, &mut manifest)?;
    }
    finish(&mut manifest);
    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(staging.join(MANIFEST), &json).context("write the manifest")?;
    // JSON holds `<` only in strings, where the escape keeps any text, such
    // as `</script>` or `<!--`, from ending the script element.
    let inline = json.replace('<', "\\u003c");
    fs::write(
        staging.join("index.html"),
        VIEWER.replace(PLACEHOLDER, &inline),
    )
    .context("write index.html")?;
    publish(&staging, &settings.out)?;

    let images: usize = manifest.pages.iter().map(|page| page.shots.len()).sum();
    println!(
        "gallery: {} page(s), {images} image(s) in {}",
        manifest.pages.len(),
        settings.out.join("index.html").display()
    );
    if !manifest.missing_glyphs.is_empty() {
        let glyphs: String = manifest.missing_glyphs.iter().collect();
        println!("warning: the screenshot font lacks {glyphs}");
    }
    if failed > 0 {
        bail!("{failed} gallery failure(s)");
    }
    Ok(())
}

/// Parse the screen sizes, and drop repeats.
fn sizes(texts: &[String]) -> Result<Vec<(String, ScreenSize)>> {
    let mut sizes: Vec<(String, ScreenSize)> = Vec::new();
    for text in texts {
        let screen = parse_screen(text).map_err(|error| anyhow!("gallery size: {error}"))?;
        if !sizes.iter().any(|(label, _)| label == text) {
            sizes.push((text.clone(), screen));
        }
    }
    Ok(sizes)
}

/// Keep the pages and the failures of the published gallery whose scripts
/// did not run, with their images. A gallery that is unreadable, or drawn
/// with another font size or scale, keeps nothing. A page whose images are
/// gone is dropped.
fn keep_others(
    out: &Path,
    staging: &Path,
    ran: &BTreeSet<String>,
    manifest: &mut Manifest,
) -> Result<()> {
    let Some(old) = fs::read_to_string(out.join(MANIFEST))
        .ok()
        .and_then(|text| serde_json::from_str::<Manifest>(&text).ok())
        .filter(|old| old.font_size == manifest.font_size && old.scale == manifest.scale)
    else {
        return Ok(());
    };
    let owners: BTreeMap<String, String> = manifest
        .pages
        .iter()
        .map(|page| (page.id.clone(), page.script.clone()))
        .collect();
    for page in old.pages {
        if ran.contains(&page.script) {
            continue;
        }
        if let Some(owner) = owners.get(&page.id) {
            manifest.failures.push(Failure {
                script: owner.clone(),
                size: None,
                message: format!("shot ID `{}` repeats in {}", page.id, page.script),
            });
            continue;
        }
        if !page
            .shots
            .iter()
            .all(|image| out.join(&image.file).is_file())
        {
            continue;
        }
        for image in &page.shots {
            fs::copy(out.join(&image.file), staging.join(&image.file))
                .with_context(|| format!("keep {}", image.file))?;
        }
        manifest.pages.push(page);
    }
    // Scripts run in name order, and a script keeps the order of its shots.
    manifest.pages.sort_by(|a, b| a.script.cmp(&b.script));
    manifest.failures.extend(
        old.failures
            .into_iter()
            .filter(|failure| !ran.contains(&failure.script)),
    );
    Ok(())
}

/// Derive the summaries of a manifest from its pages: the sizes that only
/// kept pages have, and the characters that the font lacks.
fn finish(manifest: &mut Manifest) {
    let images = manifest.pages.iter().flat_map(|page| &page.shots);
    let mut missing = BTreeSet::new();
    for image in images {
        if !manifest.sizes.contains(&image.size) {
            manifest.sizes.push(image.size.clone());
        }
        missing.extend(image.missing.iter().copied());
    }
    manifest.missing_glyphs = missing.into_iter().collect();
}

/// Read the shots of a script's return value. Luau returns an empty list as
/// an empty table, which JSON shows as an object.
fn shots(value: Option<Value>) -> Result<Vec<Shot>> {
    match value {
        Some(Value::Object(map)) if map.is_empty() => Ok(Vec::new()),
        Some(value @ Value::Array(_)) => {
            serde_json::from_value(value).context("the script returns malformed shots")
        }
        _ => bail!("the script returns no list of shots"),
    }
}

/// Check that a shot ID can name a file.
fn check_id(id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "shot ID `{id}` must be ASCII letters, digits, `-`, and `_`"
        ))
    }
}

/// Fail when the output directory holds something other than a gallery,
/// which publishing would replace.
fn check_out(out: &Path) -> Result<()> {
    let empty = || fs::read_dir(out).is_ok_and(|mut entries| entries.next().is_none());
    if !out.exists() || out.join(MANIFEST).is_file() || (out.is_dir() && empty()) {
        return Ok(());
    }
    bail!(
        "{} is not a gallery, so the run will not replace it",
        out.display()
    )
}

/// Return the path beside `dir` with a suffix on its name.
fn sibling(dir: &Path, suffix: &str) -> Result<PathBuf> {
    let name = dir
        .file_name()
        .ok_or_else(|| anyhow!("gallery directory {} has no name", dir.display()))?;
    Ok(dir.with_file_name(format!("{}.{suffix}", name.to_string_lossy())))
}

/// Replace the published gallery with the staged one. The last gallery stays
/// in place until the new one is.
fn publish(staging: &Path, out: &Path) -> Result<()> {
    if !out.exists() {
        return fs::rename(staging, out).with_context(|| format!("publish {}", out.display()));
    }
    let old = sibling(out, "old")?;
    if old.exists() {
        fs::remove_dir_all(&old).with_context(|| format!("remove {}", old.display()))?;
    }
    fs::rename(out, &old).with_context(|| format!("move {}", out.display()))?;
    if let Err(error) = fs::rename(staging, out) {
        // A rename back that fails too leaves the last gallery in `old`.
        fs::rename(&old, out).ok();
        return Err(error).with_context(|| format!("publish {}", out.display()));
    }
    fs::remove_dir_all(&old).with_context(|| format!("remove {}", old.display()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn shots_read_lists_and_reject_other_values() {
        assert!(shots(Some(json!({}))).expect("empty").is_empty());
        let capture = json!({ "width": 1, "height": 1, "styles": [], "rows": [[]] });
        let read = shots(Some(json!([{ "id": "a", "capture": capture }]))).expect("one");
        assert_eq!(read[0].id, "a");
        assert!(read[0].title.is_none());
        assert!(shots(None).is_err());
        assert!(shots(Some(json!("text"))).is_err());
        assert!(check_id("landing-page_2").is_ok());
        assert!(check_id("../up").is_err());
        assert!(check_id("").is_err());
    }

    #[test]
    fn sizes_drop_repeats_and_reject_bad_sizes() {
        let texts = ["80x24", "120x36", "80x24"].map(String::from);
        let labels: Vec<_> = sizes(&texts)
            .expect("sizes")
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        assert_eq!(labels, ["80x24", "120x36"]);
        assert!(sizes(&["80".to_owned()]).is_err());
    }

    #[test]
    fn publishing_replaces_only_a_gallery() -> Result<()> {
        let root = tempdir()?;
        let out = root.path().join("gallery");
        check_out(&out)?;
        fs::create_dir(&out)?;
        check_out(&out)?;
        fs::write(out.join("notes.txt"), "keep me")?;
        assert!(check_out(&out).is_err(), "a directory of other files stays");
        fs::write(out.join(MANIFEST), "{}")?;
        check_out(&out)?;

        let staging = root.path().join("gallery.staging");
        fs::create_dir(&staging)?;
        fs::write(staging.join(MANIFEST), "new")?;
        publish(&staging, &out)?;
        assert_eq!(fs::read_to_string(out.join(MANIFEST))?, "new");
        assert!(!out.join("notes.txt").exists());
        assert!(!staging.exists() && !root.path().join("gallery.old").exists());
        Ok(())
    }
}
