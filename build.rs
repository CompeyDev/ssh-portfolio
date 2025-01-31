use std::{env, path::PathBuf};

use anyhow::Result;
use ssh_key::{rand_core, Algorithm, EcdsaCurve, LineEnding, PrivateKey};
use vergen_gix::{BuildBuilder, CargoBuilder, Emitter, GixBuilder};

const SSH_KEY_ALGOS: &[(&'static str, Algorithm)] = &[
    ("rsa.pem", Algorithm::Rsa { hash: None }),
    ("ed25519.pem", Algorithm::Ed25519),
    ("ecdsa.pem", Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP256,
    }),
];

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");

    // Generate openSSH host keys
    let mut rng = rand_core::OsRng::default();
    let keys = SSH_KEY_ALGOS
        .iter()
        .map(|(file_name, algo)| (*file_name, PrivateKey::random(&mut rng, algo.to_owned()).map_err(anyhow::Error::from)))
        .collect::<Vec<(&str, Result<PrivateKey>)>>();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    for (file_name, key_res) in keys {
        if let Ok(ref key) = key_res {
            let path = out_dir.join(file_name);
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
