use std::{fs, process::Command};
fn main() {
    for file in [
        "../release.json",
        "../package.json",
        "tauri.conf.json",
        "../ui/build-info.js",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    let release: serde_json::Value =
        serde_json::from_str(&fs::read_to_string("../release.json").unwrap()).unwrap();
    let version = release["version"].as_str().expect("release version");
    let build = release["build"]
        .as_u64()
        .filter(|b| *b > 0)
        .expect("positive build number");
    assert_eq!(release["schema"], 1);
    assert_eq!(
        version,
        std::env::var("CARGO_PKG_VERSION").unwrap(),
        "Cargo differs from release.json"
    );
    for file in ["../package.json", "tauri.conf.json"] {
        let metadata: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(
            metadata["version"], version,
            "{file} differs from release.json"
        );
    }
    // JSON property order is immaterial; compare the generated identity itself.
    let js = fs::read_to_string("../ui/build-info.js").unwrap();
    let value = js
        .split("Object.freeze(")
        .nth(1)
        .and_then(|s| s.trim_end().strip_suffix(");"))
        .expect("generated frontend identity");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(value).unwrap(),
        release,
        "Run pnpm version:sync"
    );
    println!("cargo:rustc-env=SARGE_BUILD={build}");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
    };
    let source = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain"])
        .map(|s| if s.is_empty() { "false" } else { "true" })
        .unwrap_or("unknown");
    println!("cargo:rustc-env=SARGE_SOURCE={source}");
    println!("cargo:rustc-env=SARGE_DIRTY={dirty}");
    // Track source/index changes for diagnostics; never use them for version ordering.
    println!("cargo:rerun-if-changed=src");
    if let Some(path) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={path}");
    }
    if let Some(path) = git(&["rev-parse", "--git-path", "index"]) {
        println!("cargo:rerun-if-changed={path}");
    }
    if let Some(reference) = git(&["symbolic-ref", "--quiet", "HEAD"]) {
        if let Some(path) = git(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    tauri_build::build()
}
