//! The runner embeds the web app's build (`web/dist`). A checkout without a web build still compiles: the folder is
//! created empty, and the runner then serves a page that says how to build the UI.

fn main() {
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/dist");
    std::fs::create_dir_all(&dist).expect("create web/dist");
    println!("cargo:rerun-if-changed=../web/dist");
}
