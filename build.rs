use std::path::Path;
use std::{env, fs};

use anyhow::{Context, Result};
use vergen_gix::{BuildBuilder, CargoBuilder, Emitter, GixBuilder};

#[cfg(feature = "blog")]
const ATPROTO_LEXICON_DIR: &str = "src/atproto/lexicons";
#[cfg(feature = "blog")]
const ATPROTO_CLIENT_DIR: &str = "src/atproto";

const GENERATED_DIRS: [&str; 1] = ["src/atproto/com"];

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/atproto/lexicons");
    println!("cargo:rerun-if-changed=Cargo.toml");

    // Generate ATProto client with lexicon validation
    #[cfg(feature = "blog")]
    atrium_codegen::genapi(
        ATPROTO_LEXICON_DIR,
        ATPROTO_CLIENT_DIR,
        &[("com.whtwnd", Some("blog"))],
    )
    .unwrap();

    // Generate data for the third-party credits section
    #[cfg(feature = "credits")]
    gen_credits()?;

    // Emit the build information
    let build = BuildBuilder::all_build()?;
    let gix = GixBuilder::all_git()?;
    let cargo = CargoBuilder::all_cargo()?;
    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&gix)?
        .add_instructions(&cargo)?
        .emit_and_set()?;

    let sha = env::var("VERGEN_GIT_SHA").context("vergen did not set VERGEN_GIT_SHA")?;
    let dirty = env::var("VERGEN_GIT_DIRTY").unwrap_or_else(|_| "false".into());
    let short_sha = sha.get(..7).unwrap_or(&sha);

    // Emit the full formatted version (vX.Y.Z-COMMIT_HASH[-dirty]?)
    println!(
        "cargo:rustc-env=PKG_FULL_VERSION=v{}-{}{}",
        env!("CARGO_PKG_VERSION"),
        short_sha,
        if dirty == "true" { "-dirty" } else { "" }
    );

    // Emit the total LoC of Rust
    println!(
        "cargo:rustc-env=PKG_LOC={}",
        count_lines(Path::new("src"))? + count_lines(Path::new("crates"))?
    );

    Ok(())
}

/// Counts the number of lines of Rust source code in a given directory.
fn count_lines(dir: &Path) -> Result<usize> {
    let mut total = 0;

    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if GENERATED_DIRS.iter().any(|skip| path.ends_with(skip) || path.starts_with(skip)) {
            continue;
        }

        if path.is_dir() {
            total += count_lines(&path)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            total += fs::read_to_string(&path)?
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count();
        }
    }

    Ok(total)
}

/// Generates a JSON file containing credits for all libraries depended upon by the project
/// using `cargo-about`.
#[cfg(feature = "credits")]
fn gen_credits() -> Result<()> {
    use serde_json::Value;
    use xshell::{cmd, Shell};

    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=about.toml");
    println!("cargo:rerun-if-changed=.cargo/about-credits.hbs");

    let out_dir = env::var("OUT_DIR").context("OUT_DIR not set")?;

    let sh = Shell::new()?;
    let stdout = cmd!(
        sh,
        "cargo about generate .cargo/about-credits.hbs --offline --features localtime"
    )
    .read()
    .context("credits requires `cargo-about`")?;

    // Validate produced JSON
    serde_json::from_str::<Value>(&stdout).context("invalid JSON produced")?;

    let path = Path::new(&out_dir).join("credits.json");
    fs::write(&path, &stdout).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
