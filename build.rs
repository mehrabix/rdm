// The compiled Tailwind stylesheet is inlined at compile time (see `src/app.rs`
// via `include_str!("../assets/tailwind.css")`). `dx` runs the Tailwind compiler
// before it invokes cargo, so the file normally exists by now — but a bare
// `cargo check` would fail on a fresh clone. Create a placeholder so the crate
// always compiles, and rebuild when either the source or output CSS changes.
fn main() {
    let output = std::path::Path::new("assets/tailwind.css");
    if !output.exists() {
        let _ = std::fs::create_dir_all("assets");
        let _ = std::fs::write(
            output,
            "/* stylesheet placeholder — run `dx build`/`dx serve` to compile Tailwind */\n",
        );
    }

    println!("cargo:rerun-if-changed=tailwind.css");
    println!("cargo:rerun-if-changed=assets/tailwind.css");
}
