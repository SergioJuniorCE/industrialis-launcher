// Prism Launcher's public Microsoft application ID. Using it is subject to the
// Microsoft Identity Platform terms of use, as noted in Prism Launcher's source.
const MICROSOFT_CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";

fn main() {
    println!("cargo:rustc-env=MICROSOFT_CLIENT_ID={MICROSOFT_CLIENT_ID}");
    tauri_build::build()
}
