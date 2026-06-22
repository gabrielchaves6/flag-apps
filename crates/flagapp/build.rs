// Embed the app icon on MSVC (rc.exe available). Local GNU builds skip it;
// the released exe from CI will have the icon embedded.
fn main() {
    println!("cargo:rerun-if-changed=assets/flagapp.rc");
    println!("cargo:rerun-if-changed=assets/flag.ico");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        if std::path::Path::new("assets/flagapp.rc").exists() {
            embed_resource::compile("assets/flagapp.rc", embed_resource::NONE);
        }
    }
}
