//! Windows dependency preflight and resources, owned by the packaging crate.
use super::*;

const WEBVIEW_INSTALLER: &str = "MicrosoftEdgeWebView2RuntimeInstallerX64.exe";

fn directory(workspace: &Path, variable: &str, fallback: &str) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target/package/dependencies").join(fallback))
}

fn plugins(workspace: &Path) -> PathBuf {
    directory(workspace, "BONGOCAT_BUNDLED_PLUGINS", "plugins")
}
fn webview(workspace: &Path) -> PathBuf {
    directory(workspace, "BONGOCAT_WINDOWS_DEPENDENCIES", "").join(WEBVIEW_INSTALLER)
}
fn vc_runtime(workspace: &Path) -> PathBuf {
    env::var_os("BONGOCAT_VC_RUNTIME")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.parent().expect("development root").to_path_buf())
}

/// Reject missing offline dependencies and untrusted catalogs before building.
/// Local companions require an explicit option and an exact host build pin.
pub(super) fn verify(workspace: &Path, local_plugins: bool) -> Result<Option<String>> {
    let pin = if local_plugins {
        use std::io::Read;
        let mut bytes = Vec::new();
        fs::File::open(plugins(workspace).join("plugins.json"))?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return failure("bundled catalog is too large");
        }
        Some(bongocat_plugin_manager::sha256_of(&bytes))
    } else {
        None
    };
    let offers = bongocat_plugin_manager::verified_bundled_plugins_with_pin(
        &plugins(workspace),
        bongocat_update::PLUGIN_RELEASE_SIGNING_KEY,
        "windows-x86_64",
        env!("CARGO_PKG_VERSION"),
        pin.as_deref(),
    )
    .map_err(|e| {
        Box::new(Failure(format!(
            "provide a signed, complete BONGOCAT_BUNDLED_PLUGINS directory: {e}"
        ))) as Box<dyn std::error::Error>
    })?;
    let ids = offers
        .iter()
        .map(|offer| offer.metadata.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if ids
        != [
            "com.bongocat.todo",
            "com.bongocat.mail",
            "com.bongocat.cyberwork",
        ]
        .into_iter()
        .collect()
    {
        return failure("the Windows installer requires exactly the three companion plugins");
    }
    for file in [
        webview(workspace),
        vc_runtime(workspace).join("vcruntime140.dll"),
        vc_runtime(workspace).join("vcruntime140_1.dll"),
    ] {
        if !file.is_file() {
            return failure(format!("missing Windows dependency: {}", file.display()));
        }
        verify_microsoft_signature(&file)?;
    }
    Ok(pin)
}

fn verify_microsoft_signature(file: &Path) -> Result<()> {
    // The file path is passed as an environment value, never interpolated into
    // PowerShell source. Only the resulting exit code is used.
    let script = "$ErrorActionPreference = 'Stop'; $s = Get-AuthenticodeSignature -LiteralPath $env:BONGOCAT_VERIFY_FILE; if ($s.Status -ne 'Valid' -or $s.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)') { exit 1 }";
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("BONGOCAT_VERIFY_FILE", file)
        // PowerShell 7's inherited module path can make Windows PowerShell
        // load an incompatible Core module. Let 5.1 use its own system modules.
        .env_remove("PSModulePath")
        .status()?;
    if !status.success() {
        return failure(format!(
            "Windows dependency lacks a valid Microsoft signature: {}",
            file.display()
        ));
    }
    Ok(())
}

pub(super) fn resources(workspace: &Path, pinned_catalog: Option<&str>) -> Result<Vec<Resource>> {
    let binary_directory = if ReleaseTarget::host()? == ReleaseTarget::WindowsX86_64 {
        workspace.join("target/release")
    } else {
        workspace
            .join("target")
            .join(ReleaseTarget::WindowsX86_64.triple())
            .join("release")
    };
    let mut files = vec![
        Resource::Mapped {
            src: binary_directory
                .join(bongocat_plugin_manager::WINDOWS_PLUGIN_BROKER_FILE)
                .display()
                .to_string(),
            target: PathBuf::from(bongocat_plugin_manager::WINDOWS_PLUGIN_BROKER_FILE),
        },
        Resource::Mapped {
            src: plugins(workspace)
                .join("plugins.json")
                .display()
                .to_string(),
            target: PathBuf::from("resources/bundled-plugins/plugins.json"),
        },
        Resource::Mapped {
            src: webview(workspace).display().to_string(),
            target: PathBuf::from(format!("resources/dependencies/{WEBVIEW_INSTALLER}")),
        },
    ];
    for name in ["vcruntime140.dll", "vcruntime140_1.dll"] {
        files.push(Resource::Mapped {
            src: vc_runtime(workspace).join(name).display().to_string(),
            target: PathBuf::from(name),
        });
    }
    // Configuration inspection does not require release artifacts; the actual
    // packaging entry point verifies every dependency before building.
    if !plugins(workspace).is_dir() {
        return Ok(files);
    }
    let offers = bongocat_plugin_manager::verified_bundled_plugins_with_pin(
        &plugins(workspace),
        bongocat_update::PLUGIN_RELEASE_SIGNING_KEY,
        "windows-x86_64",
        env!("CARGO_PKG_VERSION"),
        pinned_catalog,
    )
    .map_err(|e| Box::new(Failure(e)) as Box<dyn std::error::Error>)?;
    for offer in offers {
        let path = offer.local_package_path().expect("verified local package");
        files.push(Resource::Mapped {
            src: path.display().to_string(),
            target: PathBuf::from("resources/bundled-plugins")
                .join(path.file_name().expect("verified package name")),
        });
    }
    Ok(files)
}

pub(super) fn installer_sections(workspace: &Path) -> String {
    let installer = webview(workspace).display().to_string().replace('$', "$$");
    r#"
Section "WebView2 runtime"
  ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 == ""
  ${OrIf} $0 == "0.0.0.0"
    ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
    ${If} $0 == ""
    ${OrIf} $0 == "0.0.0.0"
      InitPluginsDir
      SetOutPath "$PLUGINSDIR"
      File /oname=MicrosoftEdgeWebView2RuntimeInstallerX64.exe "@INSTALLER@"
      ExecWait '"$PLUGINSDIR\MicrosoftEdgeWebView2RuntimeInstallerX64.exe" /silent /install' $0
      ${If} $0 != 0
        DetailPrint "WebView2 setup failed ($0). BongoCat can run; retry the dependency from Settings before enabling plugins."
      ${EndIf}
    ${EndIf}
  ${EndIf}
SectionEnd

Section "un.BongoCat startup"
  ExecWait '"$INSTDIR\@BINARY@.exe" --remove-startup-item' $0
SectionEnd
"#.replace("@INSTALLER@", &installer).replace("@BINARY@", APPLICATION_BINARY)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependencies_use_app_local_crt_and_per_user_offline_webview_setup() {
        let root = Path::new("workspace");
        // A nonexistent or unsigned bundle must fail before packaging.
        assert_eq!(resources(root, None).unwrap().len(), 5);
        let files = resources(root, None).unwrap();
        assert!(files.iter().any(|file| matches!(file, Resource::Mapped { src, target }
            if target == Path::new(bongocat_plugin_manager::WINDOWS_PLUGIN_BROKER_FILE)
            && Path::new(src).file_name().is_some_and(|name| name == bongocat_plugin_manager::WINDOWS_PLUGIN_BROKER_FILE))));
        let script = installer_sections(root);
        assert!(script.contains("HKCU"));
        assert!(script.contains("WOW6432Node"));
        assert!(script.contains("/silent /install"));
        assert!(script.contains("--remove-startup-item"));
        assert!(!script.contains("runas"));
    }
}
