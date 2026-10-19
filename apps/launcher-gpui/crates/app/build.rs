fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let numeric = version
        .split('.')
        .map(|s| {
            s.parse::<u16>()
                .expect("Windows version component must fit u16")
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(",");
    let icon = std::path::Path::new("assets/icon.ico")
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
        .replace('\\', "\\\\");
    let rc = format!(
        r#"
#include <windows.h>
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric},0
PRODUCTVERSION {numeric},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904b0"
  BEGIN
   VALUE "CompanyName", "Industrialis\0"
   VALUE "FileDescription", "Industrialis Launcher (GPUI)\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "ProductName", "Industrialis Launcher\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "OriginalFilename", "industrialis.exe\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x0409, 1200
 END
END
"#
    );
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("launcher.rc");
    std::fs::write(&path, rc).unwrap();
    embed_resource::compile(path, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
