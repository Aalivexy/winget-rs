fn main() {
    let winmd = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("Microsoft.Management.Deployment.winmd");

    windows_bindgen::bindgen([
        "--in",
        "default",
        "--in",
        winmd.to_str().expect("non-UTF-8 path for .winmd file"),
        "--out",
        "src/bindings.rs",
        "--filter",
        "Microsoft.Management.Deployment",
        "--specific-deps",
        "--reference",
        "windows,skip-root,Windows.Foundation",
        "--reference",
        "windows,skip-root,Windows.System",
        "--reference",
        "windows,skip-root,Windows.Security.Cryptography.Certificates",
    ])
    .unwrap();
}
