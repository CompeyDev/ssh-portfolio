use anyhow::Result;
use vergen_gix::{BuildBuilder, CargoBuilder, Emitter, GixBuilder};

#[cfg(feature = "blog")]
const ATPROTO_LEXICON_DIR: &str = "src/atproto/lexicons";
#[cfg(feature = "blog")]
const ATPROTO_CLIENT_DIR: &str = "src/atproto";

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

    // Emit the build information
    let build = BuildBuilder::all_build()?;
    let gix = GixBuilder::all_git()?;
    let cargo = CargoBuilder::all_cargo()?;
    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&gix)?
        .add_instructions(&cargo)?
        .emit_and_set()?;

    // Emit the full formatted version (vX.Y.Z-COMMIT_HASH[-dirty]?)
    println!(
        "cargo:rustc-env=PKG_FULL_VERSION=v{}-{}{}",
        env!("CARGO_PKG_VERSION"),
        &env!("VERGEN_GIT_SHA")[..7],
        if env!("VERGEN_GIT_DIRTY") == "true" { "-dirty" } else { "" }
    );

    Ok(())
}
