//! Embeds the app icon (`packaging/windows/app-icon.rc`) in the Windows
//! program.

const RESOURCES: &str = "packaging/windows/app-icon.rc";

fn main() {
    println!("cargo:rerun-if-changed={RESOURCES}");
    println!("cargo:rerun-if-changed=packaging/windows/app-icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    match embed_resource::compile(RESOURCES, embed_resource::NONE) {
        embed_resource::CompilationResult::Ok | embed_resource::CompilationResult::NotWindows => {}
        // A check of the Windows code from another system may have no
        // resource compiler; it builds no program to ship.
        embed_resource::CompilationResult::NotAttempted(why) if !cfg!(windows) => {
            println!("cargo:warning=the app icon is not embedded: {why}");
        }
        failed => panic!("cannot embed the app icon: {failed}"),
    }
}
