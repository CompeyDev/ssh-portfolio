use std::{env, path::PathBuf};

use anyhow::Result;
use ssh_key::{rand_core, Algorithm, EcdsaCurve, LineEnding, PrivateKey};
use vergen_gix::{BuildBuilder, CargoBuilder, Emitter, GixBuilder};

const SSH_KEY_ALGOS: &[Algorithm] = &[
    Algorithm::Rsa { hash: None },
    Algorithm::Ed25519,
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP256,
    },
];

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    // Generate openSSH host keys
    let mut rng = rand_core::OsRng::default();
    let keys = SSH_KEY_ALGOS
        .iter()
        .map(|algo| PrivateKey::random(&mut rng, algo.to_owned()).map_err(anyhow::Error::from))
        .collect::<Vec<Result<PrivateKey>>>();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    for key_res in keys {
        if let Ok(ref key) = key_res {
            let path = out_dir.join(format!("{}.pem", key.algorithm().as_str()));
            if path.exists() {
                println!("cargo:warning=Skipping existing host key: {:?}", path.file_stem());
                continue;
            }
            
            key.write_openssh_file(&path, LineEnding::default())?;
        } else {
            println!("cargo:warning=Failed to generate host key: {:?}", key_res);
        }
    }

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
