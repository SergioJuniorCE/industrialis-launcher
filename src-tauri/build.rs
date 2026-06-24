use std::path::Path;

fn read_client_id() -> String {
    if let Ok(id) = std::env::var("MICROSOFT_CLIENT_ID") {
        let trimmed = id.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }

    let path = Path::new("microsoft-client-id");
    if path.exists() {
        if let Ok(raw) = std::fs::read_to_string(path) {
            let trimmed = raw.trim().to_string();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                return trimmed;
            }
        }
    }

    String::new()
}

fn main() {
    let client_id = read_client_id();
    println!("cargo:rustc-env=MICROSOFT_CLIENT_ID={client_id}");
    println!("cargo:rerun-if-env-changed=MICROSOFT_CLIENT_ID");
    println!("cargo:rerun-if-changed=microsoft-client-id");
    tauri_build::build()
}
