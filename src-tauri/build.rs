// Copyright (c) 2026 Remgrandt Works. All rights reserved.

use std::env;
use std::path::PathBuf;

fn main() {
    configure_libvips_linking();
    tauri_build::build()
}

fn configure_libvips_linking() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !matches!(target_os.as_str(), "windows" | "macos") {
        return;
    }

    for dir in libvips_link_search_dirs() {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }

    if target_os == "windows" {
        println!("cargo:rustc-link-lib=dylib=delayimp");
        println!("cargo:rustc-link-arg=/DELAYLOAD:libvips-42.dll");
        println!("cargo:rustc-link-arg=/DELAYLOAD:libglib-2.0-0.dll");
        println!("cargo:rustc-link-arg=/DELAYLOAD:libgobject-2.0-0.dll");
    }

    if target_os == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Resources/resources/libvips");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Resources/libvips");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../Resources/resources/libvips");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../Resources/libvips");
    }
}

fn libvips_link_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = env::var_os("OACURATOR_VIPS_LIB_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        dirs.push(path);
    }

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let resource_dir = PathBuf::from(manifest_dir)
            .join("resources")
            .join("libvips");
        dirs.push(resource_dir.join("lib"));
        dirs.push(resource_dir);
    }

    dirs.into_iter().filter(|dir| dir.is_dir()).collect()
}
