fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = std::path::Path::new("../../assets/icons/app.ico")
            .canonicalize()
            .expect("application icon missing");
        let resource =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("gulu.rc");
        // rc.exe expects a regular path, not Windows' extended-length prefix.
        let icon_path = icon.to_string_lossy();
        let icon_path = icon_path
            .strip_prefix(r"\\?\")
            .unwrap_or(&icon_path)
            .replace('\\', "/");
        std::fs::write(
            &resource,
            format!("#pragma code_page(65001)\n1 ICON \"{icon_path}\"\n"),
        )
        .unwrap();
        embed_resource::compile(&resource, embed_resource::NONE)
            .manifest_required()
            .expect("Windows icon resource compilation failed");
    }
}
