// Embed assets/icon.ico as the exe's icon (needs rc.exe from the Windows SDK).
// The .ico is generated from assets/icon*.svg; see assets/README.md.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("assets/app.rc", embed_resource::NONE).manifest_optional().unwrap();
}
