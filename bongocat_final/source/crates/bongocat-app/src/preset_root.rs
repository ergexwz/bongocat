//! Where the model packages this build ships live.
//!
//! A packaged build carries them inside its own bundle; a Development run finds
//! the repository's own resources. Both are resolved from the executable rather
//! than from the working directory, so the product behaves the same however it
//! was launched.

use super::*;

pub(crate) fn preset_root() -> PathBuf {
    let executable = env::current_exe().expect("resolve the product executable");
    preset_root_for_executable(&executable, bongocat_app::BUILD_ENVIRONMENT)
}

pub(super) fn preset_root_for_executable(
    executable: &Path,
    environment: bongocat_config::BuildEnvironment,
) -> PathBuf {
    if let Some(root) = bundled_preset_root(executable)
        && (root.is_dir() || environment == bongocat_config::BuildEnvironment::Production)
    {
        return root;
    }
    // A broken installer must surface missing resources, even on the build
    // machine. Production never falls back to an absolute developer checkout.
    if environment == bongocat_config::BuildEnvironment::Production {
        return executable
            .parent()
            .expect("executable directory")
            .join("resources/models");
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .join("resources/models")
}

#[cfg(target_os = "macos")]
pub(crate) fn bundled_preset_root(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    if macos.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    Some(contents.join("Resources/models"))
}

#[cfg(target_os = "windows")]
pub(crate) fn bundled_preset_root(executable: &Path) -> Option<PathBuf> {
    executable_relative_preset_root(executable)
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn executable_relative_preset_root(executable: &Path) -> Option<PathBuf> {
    Some(executable.parent()?.join("resources/models"))
}
