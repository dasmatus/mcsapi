//! Embeds every SVG under `icons/` so an app can serve them without shipping
//! an assets directory next to its binary.

use std::{
    env,
    error::Error,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

fn collect(
    directory: &Path,
    prefix: &str,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_dir() {
            collect(&path, &format!("{prefix}{name}/"), out)?;
        } else if path.extension().is_some_and(|extension| extension == "svg") {
            out.push((format!("{prefix}{name}"), path.canonicalize()?));
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let icons = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("icons");
    println!("cargo::rerun-if-changed={}", icons.display());

    let mut files = Vec::new();
    collect(&icons, "icons/", &mut files)?;
    // Sorted so `svg` can binary-search and the generated file is stable.
    files.sort();

    let mut source = String::from("pub(crate) static EMBEDDED: &[(&str, &[u8])] = &[\n");
    for (name, path) in &files {
        writeln!(
            source,
            "    ({name:?}, include_bytes!({:?})),",
            path.display().to_string()
        )?;
    }
    source.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("embedded.rs"),
        source,
    )?;
    Ok(())
}
