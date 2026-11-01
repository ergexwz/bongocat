//! Resolving the model packages this build ships.

use super::*;

#[cfg(target_os = "macos")]
#[test]
fn bundled_preset_models_resolve_from_contents_resources() {
    assert_eq!(
        bundled_preset_root(Path::new(
            "/Applications/BongoCat.app/Contents/MacOS/bongocat-app"
        )),
        Some(PathBuf::from(
            "/Applications/BongoCat.app/Contents/Resources/models"
        ))
    );
    assert_eq!(
        bundled_preset_root(Path::new("/tmp/target/release/bongocat-app")),
        None
    );
}

#[test]
fn executable_relative_preset_models_resolve_next_to_a_product_executable() {
    assert_eq!(
        executable_relative_preset_root(Path::new("/Applications/BongoCat/bongocat-app.exe")),
        Some(PathBuf::from("/Applications/BongoCat/resources/models"))
    );
}

#[cfg(target_os = "windows")]
#[test]
fn a_missing_production_bundle_never_uses_the_developer_checkout() {
    let executable = Path::new("C:\\用户安装 目录\\BongoCat\\bongocat-app.exe");
    assert_eq!(
        crate::preset_root::preset_root_for_executable(
            executable,
            bongocat_config::BuildEnvironment::Production
        ),
        PathBuf::from("C:\\用户安装 目录\\BongoCat\\resources\\models")
    );
}
