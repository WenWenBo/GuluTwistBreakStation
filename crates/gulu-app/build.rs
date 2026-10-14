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
        let version = std::env::var("CARGO_PKG_VERSION").unwrap();
        let numbers = format!("{},0", version.replace('.', ","));
        std::fs::write(
            &resource,
            format!(
                r#"#pragma code_page(65001)
1 ICON "{icon_path}"
1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904B0"
  BEGIN
   VALUE "FileDescription", "GuluTwist Break Station\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "ProductName", "GuluTwist Break Station\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "OriginalFilename", "gulu-twist.exe\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x409, 1200
 END
END
"#
            ),
        )
        .unwrap();
        embed_resource::compile(&resource, embed_resource::NONE)
            .manifest_required()
            .expect("Windows icon resource compilation failed");
    }
}
