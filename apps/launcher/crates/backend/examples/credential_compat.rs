//! Synthetic-profile interoperability fixture; never prints decrypted secrets.
use industrialis_backend::{credentials,paths::LauncherPaths};
use std::io::Read;
fn main()->anyhow::Result<()> {
    let paths=LauncherPaths::new(std::env::args_os().nth(1).ok_or_else(||anyhow::anyhow!("fixture profile required"))?.into());
    let mut input=String::new();std::io::stdin().read_to_string(&mut input)?;let expected=input.trim();
    let encrypted=std::fs::read_to_string(paths.base().join("electron-cipher.txt"))?;
    anyhow::ensure!(credentials::decrypt(&paths,encrypted.trim())?==expected,"Electron credential interoperability failed");
    let rt=tokio::runtime::Runtime::new()?;
    let encrypted=rt.block_on(credentials::encrypt(&paths,expected.into()))?;
    std::fs::write(paths.base().join("rust-cipher.txt"),encrypted)?;
    println!("Credential interoperability verified");Ok(())
}
