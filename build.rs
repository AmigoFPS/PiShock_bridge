fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        resource.set("ProductName", "Pishock_bridge");
        resource.set("FileDescription", "VRChat contacts to PiShock devices");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=Icon not embedded in the exe: {error}");
        }
    }
}
