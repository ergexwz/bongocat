use std::{io::Write, path::PathBuf};

pub const ID: &str = "com.example.host-fixture";

pub fn package(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let binary = PathBuf::from(
        std::env::var_os("BONGOCAT_TEST_FIXTURE").unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(format!(
                    "plugin-test-fixture{}",
                    std::env::consts::EXE_SUFFIX
                ))
                .into_os_string()
        }),
    );
    let bytes = std::fs::read(&binary).unwrap_or_else(|error| {
        panic!(
            "required host fixture {} is unavailable: {error}; build plugin-test-fixture first",
            binary.display()
        )
    });
    let mut manifest = serde_json::json!({"metadata": {
        "id": ID, "name": "Host fixture", "description": "Deterministic host lifecycle fixture.",
        "version": "1.0.0", "app_versions": ">=2.5.3", "protocol_version": 1,
        "entry": "plugin-test-fixture", "icon": {"type": "local_image", "value": "resources/icon.png"},
        "capabilities": ["model_ui", "configuration", "persistent_state", "localized_views_v1"],
        "localizations": {}
    }});
    edit(&mut manifest);
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
    archive.start_file("plugin.json", options).unwrap();
    archive
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    archive
        .start_file(
            format!("plugin-test-fixture{}", std::env::consts::EXE_SUFFIX),
            options,
        )
        .unwrap();
    archive.write_all(&bytes).unwrap();
    archive.start_file("resources/icon.png", options).unwrap();
    archive
        .write_all(include_bytes!(
            "../../../../../../bongocat_plugins/plugins/todo/resources/icon.png"
        ))
        .unwrap();
    archive.finish().unwrap().into_inner()
}
