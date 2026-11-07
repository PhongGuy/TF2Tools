//! Embeds the TF2Tools icon into tf2tools.exe on Windows; other targets need no resources.
//! Also rebuilds when `licenses/` changes, so a new notice file is embedded without a clean.

// A build script reports failure by panicking, so a missing icon resource fails the build.
#[allow(clippy::expect_used)]
fn main() {
    println!("cargo:rerun-if-changed=windows/tf2tools.rc");
    println!("cargo:rerun-if-changed=../../licenses");
    println!("cargo:rerun-if-changed=../../packaging/icons/icon.ico");
    // Build scripts run on the host, so the target comes from Cargo's environment, not cfg!.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile(
            "windows/tf2tools.rc",
            embed_resource::ParamsIncludeDirs(["../../packaging/icons"]),
        )
        .manifest_optional()
        .expect("compile the Windows icon resource");
    }
}
