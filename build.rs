fn main() {
    generate_embedded_ui_assets();

    #[cfg(windows)]
    {
        compile_windows_resources();
    }
}

#[cfg(windows)]
fn compile_windows_resources() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("packaging/windows/amele.ico");
    res.compile().unwrap();
}

fn generate_embedded_ui_assets() {
    let manifest_dir = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"),
    );
    let ui_dir = manifest_dir.join("ui");
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let out_file = out_dir.join("ui_assets.rs");
    let mut files = Vec::new();

    collect_ui_files(&ui_dir, &ui_dir, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut generated =
        String::from("pub fn get(path: &str) -> Option<&'static [u8]> {\n    match path {\n");
    for (route, path) in files {
        let include_path = path.to_string_lossy().replace('\\', "/");
        generated.push_str("        ");
        generated.push_str(&format!("{route:?}"));
        generated.push_str(" => Some(include_bytes!(");
        generated.push_str(&format!("{include_path:?}"));
        generated.push_str(").as_slice()),\n");
    }
    generated.push_str("        _ => None,\n    }\n}\n");

    std::fs::write(out_file, generated).expect("embedded UI asset map can be written");
    println!("cargo:rerun-if-changed={}", ui_dir.display());
}

fn collect_ui_files(
    root: &std::path::Path,
    dir: &std::path::Path,
    files: &mut Vec<(String, std::path::PathBuf)>,
) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|err| {
        panic!(
            "UI asset directory cannot be read: {}: {err}",
            dir.display()
        )
    });

    for entry in entries {
        let entry = entry.expect("UI asset directory entry can be read");
        let path = entry.path();
        let file_type = entry.file_type().expect("UI asset file type can be read");
        if file_type.is_dir() {
            collect_ui_files(root, &path, files);
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("UI asset path is under UI root")
                .to_string_lossy()
                .replace('\\', "/");
            files.push((format!("/{relative}"), path));
            println!(
                "cargo:rerun-if-changed={}",
                files.last().unwrap().1.display()
            );
        }
    }
}
// dostum ben hayal kırıklığıyla savaşıyorum
