//! Points the compile at the renderer bundle and, when the fetch script has
//! unpacked one, the rayfin-local binary to embed in the executable.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
  let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
  let target = env::var("TARGET").unwrap_or_default();
  let sidecar_name = if target.contains("windows") {
    format!("rayfin-local-{target}.exe")
  } else {
    format!("rayfin-local-{target}")
  };
  let sidecar = manifest.join("binaries").join(sidecar_name);
  let assets = manifest.join("assets");
  let dest = assets.join("rayfin-local.bin");
  fs::create_dir_all(&assets).ok();
  let embedded = sidecar.is_file() && fs::metadata(&sidecar).map(|meta| meta.len() > 0).unwrap_or(false);
  if embedded {
    fs::copy(&sidecar, &dest).expect("copy rayfin-local into the embed path");
  } else {
    fs::write(&dest, []).ok();
  }
  println!("cargo:rustc-env=RAYFIN_LOCAL_EMBEDDED={}", if embedded { "1" } else { "0" });
  println!("cargo:rerun-if-changed={}", sidecar.display());

  let index = manifest.join("../dist/index.html");
  if !index.exists() {
    fs::create_dir_all(index.parent().unwrap()).ok();
    fs::write(
      &index,
      "<!doctype html><html><head><meta charset=\"utf-8\"><title>Fabricator</title></head><body><p>Build the renderer with npm run build:renderer.</p></body></html>",
    )
    .ok();
  }
  println!("cargo:rerun-if-changed=../dist/index.html");
}
