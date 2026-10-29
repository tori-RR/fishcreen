fn main() {
    // Windows' compiled icon resource must be regenerated when the ICO changes.
    println!("cargo:rerun-if-changed=../assets/fishcreen.ico");
    tauri_build::build();
}
