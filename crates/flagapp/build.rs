// Embed the app icon on every Windows target. MSVC uses rc.exe; GNU uses
// windres (shipped with the MinGW toolchain). Embedding it here means the icon
// is always available via LoadIconW(hinstance, 1) for the tray, About window,
// dialogs and taskbar — instead of depending on flag.ico sitting next to the exe.
fn main() {
    println!("cargo:rerun-if-changed=assets/flagapp.rc");
    println!("cargo:rerun-if-changed=assets/flag.ico");
    if std::path::Path::new("assets/flagapp.rc").exists() {
        embed_resource::compile("assets/flagapp.rc", embed_resource::NONE);
    }
}
