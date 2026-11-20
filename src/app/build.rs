fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=icons/icon.ico");
        winresource::WindowsResource::new()
            .set_icon("icons/icon.ico")
            .set("ProductName", "MusicFS")
            .set("FileDescription", "MusicFS Desktop")
            .set("OriginalFilename", "musicfs-desktop.exe")
            .compile()
            .expect("Could not build the Windows application resource");
    }
}
