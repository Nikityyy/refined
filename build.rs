fn main() {
    println!("cargo:rerun-if-changed=installer/refined.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("installer/refined.ico")
            .set("ProductName", "Refined")
            .set("FileDescription", "Refined - AI Text Refiner")
            .set("LegalCopyright", "Copyright (c) 2026 Nikita Berger")
            .set("OriginalFilename", "Refined.exe")
            .compile()
            .expect("Could not compile the Windows icon resource");
    }
}
