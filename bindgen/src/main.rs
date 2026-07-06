fn main() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("src")
        .join("bindings.rs");

    windows_bindgen::bindgen([
        "--in",
        "default",
        "--in",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "\\Microsoft.Management.Deployment.winmd"
        ),
        "--out",
        out.to_str().expect("non-UTF-8 path for output file"),
        "--filter",
        "Microsoft.Management.Deployment",

        "--reference",
        "windows,skip-root,Windows.Foundation",
        "--reference",
        "windows,skip-root,Windows.System",
        "--reference",
        "windows,skip-root,Windows.Security.Cryptography.Certificates",
    ])
    .unwrap();
}
