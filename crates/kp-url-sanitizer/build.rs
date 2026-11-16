use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

fn collect(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            files.push(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest.join("config");
    let mut files = Vec::new();
    collect(&root, &root, &mut files)?;
    files.sort();

    let mut generated = String::from("pub(crate) const EMBEDDED_CONFIGS: &[(&str, &str)] = &[\n");
    for relative in &files {
        let path = root.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        generated.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            relative.display().to_string(),
            path.display().to_string()
        ));
    }
    generated.push_str("];\n");

    let out = PathBuf::from(std::env::var("OUT_DIR")?).join("embedded_configs.rs");
    fs::write(out, generated)?;
    println!("cargo:rerun-if-changed={}", root.display());
    Ok(())
}
