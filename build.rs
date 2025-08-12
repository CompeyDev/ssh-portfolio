use std::{env, path::PathBuf};

use anyhow::Result;
use ssh_key::{rand_core, Algorithm, EcdsaCurve, LineEnding, PrivateKey};
use vergen_gix::{BuildBuilder, CargoBuilder, Emitter, GixBuilder};

#[cfg(feature = "blog")]
const ATPROTO_LEXICON_DIR: &str = "src/atproto/lexicons";
#[cfg(feature = "blog")]
const ATPROTO_CLIENT_DIR: &str = "src/atproto";
const SSH_KEY_ALGOS: &[(&'static str, Algorithm)] = &[
    ("rsa.pem", Algorithm::Rsa { hash: None }),
    ("ed25519.pem", Algorithm::Ed25519),
    (
        "ecdsa.pem",
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
    ),
];

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/atproto/lexicons");

    // Generate openSSH host keys
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut rng = rand_core::OsRng::default();
    for (file_name, algo) in SSH_KEY_ALGOS {
        let path = out_dir.join(file_name);
        if path.exists() {
            println!(
                "cargo:warning=Skipping existing host key: {:?}",
                path.file_stem().unwrap()
            );
            continue;
        }

        let key = PrivateKey::random(&mut rng, algo.to_owned()).map_err(anyhow::Error::from)?;
        key.write_openssh_file(&path, LineEnding::default())?;
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
