use std::env;

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
    println!("cargo:rerun-if-changed=patches/");

    #[cfg(feature = "blog")]
    {
        println!("cargo:rerun-if-env-changed=SKIP_PATCH_CRATE");
        if env::var("SKIP_PATCH_CRATE").is_err() {
            patch_crate::run().expect("Failed while patching");
        }
    }

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
        .emit()
}
