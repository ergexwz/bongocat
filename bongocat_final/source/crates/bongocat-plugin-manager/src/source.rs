//! Package discovery never installs or executes the advertised code.
use crate::{Catalog, CatalogEntry, catalog, install};
use bongocat_plugin_sdk::PluginMetadata;
use cargo_packager_updater::semver::{Version, VersionReq};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug)]
pub enum PluginSource {
    InstalledOnly,
    Development {
        catalog: PathBuf,
        packages: PathBuf,
    },
    Production {
        owner: String,
        repository: String,
        public_key: String,
        platform: String,
        bundled_directory: Option<PathBuf>,
        /// Exact catalog digest compiled into a local companion build. Online
        /// catalogs still require the release signature.
        bundled_catalog_sha256: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct AvailablePlugin {
    pub metadata: PluginMetadata,
    /// Where the card reads the same PNG the package carries.
    ///
    /// This is catalog projection state rather than package manifest data, so it
    /// stays beside the metadata instead of entering the plugin protocol.
    pub icon_url: String,
    pub digest: String,
    location: PackageLocation,
}
#[derive(Clone, Debug)]
enum PackageLocation {
    Local(PathBuf),
    Https(String),
}

impl AvailablePlugin {
    /// Local package path after catalog/path verification, for installer staging.
    pub fn local_package_path(&self) -> Option<&Path> {
        match &self.location {
            PackageLocation::Local(path) => Some(path),
            PackageLocation::Https(_) => None,
        }
    }
    /// Replace a remote catalog icon with a persistent local copy.
    ///
    /// Development catalogs already point at a package-local file, so they are
    /// left untouched. Production performs this off the service's UI-facing
    /// loop; a failure keeps the remote URL so the card can still try it.
    pub fn cache_icon(&mut self, plugin_dir: &Path, stop: &AtomicBool) -> Result<(), String> {
        if !self.icon_url.starts_with("https://") {
            return Ok(());
        }
        let cached = crate::icons::cache_remote_icon(
            plugin_dir,
            &self.metadata.id,
            &self.metadata.version,
            &self.icon_url,
            stop,
        )?;
        self.icon_url = cached.to_string_lossy().into_owned();
        Ok(())
    }

    pub fn read(&self, stop: &AtomicBool) -> Result<Vec<u8>, String> {
        let bytes = match &self.location {
            PackageLocation::Local(path) => {
                install::read_bounded(path, catalog::MAX_DOWNLOAD_BYTES)
                    .map_err(|_| "local plugin package is unavailable".to_owned())?
            }
            PackageLocation::Https(url) => {
                catalog::download_blocking(url, stop).map_err(|error| error.to_string())?
            }
        };
        if catalog::sha256_of(&bytes) != self.digest {
            return Err("plugin package digest changed; refresh the catalog".into());
        }
        let manifest = install::inspect_package(&bytes).map_err(|error| error.to_string())?;
        if manifest.metadata.id != self.metadata.id
            || manifest.metadata.version != self.metadata.version
        {
            return Err("plugin package does not match the catalog".into());
        }
        Ok(bytes)
    }
}

impl PluginSource {
    /// Seed verified bundled packages exactly once without enabling them or
    /// reinstalling something the user subsequently uninstalled.
    pub fn install_bundled(
        &self,
        manager: &mut crate::PluginManager,
        app: &str,
        stop: &AtomicBool,
    ) -> Result<(), String> {
        let Self::Production {
            bundled_directory: Some(directory),
            bundled_catalog_sha256,
            public_key,
            platform,
            ..
        } = self
        else {
            return Ok(());
        };
        if !directory.exists() {
            return Ok(());
        }
        let offers = verified_bundled_plugins_with_pin(
            directory,
            public_key,
            platform,
            app,
            bundled_catalog_sha256.as_deref(),
        )?;
        let marker = manager.plugin_dir().join("bundled-installed.json");
        let mut seen: Vec<String> = match install::read_bounded(&marker, 16 * 1024) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| "invalid bundled installation record")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.to_string()),
        };
        if seen.len() > 32 {
            return Err("invalid bundled installation record".into());
        }
        for offer in offers {
            if seen.contains(&offer.metadata.id) {
                continue;
            }
            if manager.installed_version(&offer.metadata.id).is_none() {
                let bytes = offer.read(stop)?;
                manager
                    .install_verified(&bytes, &offer.digest)
                    .map_err(|e| e.to_string())?;
            }
            seen.push(offer.metadata.id);
            let mut file =
                atomic_write_file::AtomicWriteFile::open(&marker).map_err(|e| e.to_string())?;
            serde_json::to_writer(&mut file, &seen).map_err(|e| e.to_string())?;
            file.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn discover(&self, app: &str, stop: &AtomicBool) -> Result<Vec<AvailablePlugin>, String> {
        let mut candidates = Vec::new();
        match self {
            Self::InstalledOnly => {}
            Self::Development { catalog, packages } => {
                let catalog = match fs::read(catalog) {
                    Ok(bytes) => serde_json::from_slice::<Catalog>(&bytes)
                        .map_err(|error| error.to_string())?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(Vec::new());
                    }
                    Err(error) => return Err(error.to_string()),
                };
                for entry in catalog.plugins {
                    if stop.load(Ordering::Acquire) {
                        return Err("catalog refresh cancelled".into());
                    }
                    let package_name = entry
                        .downloads
                        .values()
                        .next()
                        .cloned()
                        .unwrap_or_else(|| format!("{}-{}.zip", entry.id, entry.version));
                    let package = packages.join(package_name);
                    let bytes = install::read_bounded(&package, catalog::MAX_DOWNLOAD_BYTES)
                        .map_err(|error| error.to_string())?;
                    let manifest =
                        install::inspect_package(&bytes).map_err(|error| error.to_string())?;
                    if manifest.metadata.id != entry.id
                        || manifest.metadata.version != entry.version
                    {
                        return Err("local catalog and package disagree".into());
                    }
                    candidates.push(AvailablePlugin {
                        metadata: from_catalog(&entry, String::new(), catalog::sha256_of(&bytes))
                            .metadata,
                        icon_url: entry.icon_url,
                        digest: catalog::sha256_of(&bytes),
                        location: PackageLocation::Local(package),
                    });
                }
            }
            Self::Production {
                owner,
                repository,
                public_key,
                platform,
                ..
            } => {
                let catalog = catalog::fetch_catalog_blocking(owner, repository, public_key, stop)
                    .map_err(|error| error.to_string())?;
                for entry in catalog.plugins {
                    if entry.protocol_version != bongocat_plugin_sdk::PROTOCOL_VERSION {
                        continue;
                    }
                    let Some(url) = entry.downloads.get(platform) else {
                        continue;
                    };
                    // A package is only ever the one the signed catalog
                    // describes, so an entry without a usable digest is a
                    // catalog that cannot be acted on rather than one that is
                    // installed unverified.
                    let digest = entry
                        .sha256
                        .clone()
                        .filter(|digest| {
                            digest.len() == 64
                                && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                        .ok_or("signed catalog lacks a package digest")?;
                    candidates.push(from_catalog(
                        &entry,
                        url.clone(),
                        digest.to_ascii_lowercase(),
                    ));
                }
            }
        }
        select_candidates(candidates, app)
    }
}

/// Used by both the installer preflight and the runtime: a local ZIP is trusted
/// only through its digest in the release-key-signed catalog.
pub fn verified_bundled_plugins(
    directory: &Path,
    public_key: &str,
    platform: &str,
    app: &str,
) -> Result<Vec<AvailablePlugin>, String> {
    verified_bundled_plugins_with_pin(directory, public_key, platform, app, None)
}

/// The pin must come from the application build, never an adjacent file or a
/// runtime environment variable. This does not change online catalog trust.
pub fn verified_bundled_plugins_with_pin(
    directory: &Path,
    public_key: &str,
    platform: &str,
    app: &str,
    compiled_catalog_sha256: Option<&str>,
) -> Result<Vec<AvailablePlugin>, String> {
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    let bytes = install::read_bounded(
        &directory.join(catalog::CATALOG_NAME),
        catalog::MAX_CATALOG_BYTES,
    )
    .map_err(|e| e.to_string())?;
    let signed = if let Some(pin) = compiled_catalog_sha256 {
        if pin.len() != 64
            || !pin.bytes().all(|c| c.is_ascii_hexdigit())
            || !catalog::sha256_of(&bytes).eq_ignore_ascii_case(pin)
        {
            return Err("bundled catalog differs from this application build".into());
        }
        serde_json::from_slice::<Catalog>(&bytes).map_err(|e| e.to_string())?
    } else {
        catalog::parse_catalog(&bytes, public_key).map_err(|e| e.to_string())?
    };
    let mut offers = Vec::new();
    for entry in signed.plugins {
        let Some(download) = entry.downloads.get(platform) else {
            continue;
        };
        let name = download
            .rsplit('/')
            .next()
            .ok_or("invalid bundled package name")?;
        if !name.ends_with(".zip")
            || name.len() > 255
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err("invalid bundled package name".into());
        }
        let package = directory
            .join(name)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if package.parent() != Some(directory.as_path()) {
            return Err("bundled package escapes its directory".into());
        }
        let digest = entry
            .sha256
            .clone()
            .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or("signed catalog lacks a package digest")?;
        let mut offer = from_catalog(&entry, String::new(), digest.to_ascii_lowercase());
        offer.location = PackageLocation::Local(package);
        // Preflight integrity and package identity now, before any installation.
        offer.read(&AtomicBool::new(false))?;
        offers.push(offer);
    }
    select_candidates(offers, app)
}

fn select_candidates(
    candidates: Vec<AvailablePlugin>,
    app: &str,
) -> Result<Vec<AvailablePlugin>, String> {
    let app = Version::parse(app).map_err(|error| error.to_string())?;
    let mut newest: BTreeMap<String, (Version, AvailablePlugin)> = BTreeMap::new();
    for candidate in candidates {
        if !VersionReq::parse(&candidate.metadata.app_versions).is_ok_and(|req| req.matches(&app)) {
            continue;
        }
        let version =
            Version::parse(&candidate.metadata.version).map_err(|error| error.to_string())?;
        if newest
            .get(&candidate.metadata.id)
            .is_none_or(|(previous, _)| version > *previous)
        {
            newest.insert(candidate.metadata.id.clone(), (version, candidate));
        }
    }
    Ok(newest
        .into_values()
        .map(|(_, candidate)| candidate)
        .collect())
}

fn from_catalog(entry: &CatalogEntry, url: String, digest: String) -> AvailablePlugin {
    AvailablePlugin {
        metadata: PluginMetadata {
            id: entry.id.clone(),
            name: entry.name.clone(),
            description: entry.description.clone(),
            version: entry.version.clone(),
            app_versions: entry.app_versions.clone(),
            protocol_version: entry.protocol_version,
            entry: String::new(),
            icon: entry.icon.clone(),
            capabilities: Vec::new(),
            // The catalog carries the plugin's own localized copy, so an
            // available card reads the same words an installed one does.
            localizations: entry.localizations.clone(),
        },
        icon_url: entry.icon_url.clone(),
        digest,
        location: PackageLocation::Https(url),
    }
}

pub fn is_newer(available: &str, installed: &str) -> bool {
    match (Version::parse(available), Version::parse(installed)) {
        (Ok(a), Ok(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn package(id: &str, version: &str, app_versions: &str) -> Vec<u8> {
        let manifest = crate::PluginManifest {
            metadata: bongocat_plugin_sdk::PluginMetadata {
                id: id.into(),
                name: "Test plugin".into(),
                description: "A test fixture plugin.".into(),
                version: version.into(),
                app_versions: app_versions.into(),
                protocol_version: bongocat_plugin_sdk::PROTOCOL_VERSION,
                entry: "plugin".into(),
                icon: bongocat_plugin_sdk::PluginIcon::Emoji("A".into()),
                capabilities: Vec::new(),
                localizations: Default::default(),
            },
        };
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive
            .start_file(
                crate::MANIFEST_FILE,
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive
            .write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        let entry = crate::loader::entry_path(Path::new(""), "plugin")
            .to_string_lossy()
            .into_owned();
        archive
            .start_file(entry, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"fixture: never executed").unwrap();
        archive.finish().unwrap().into_inner()
    }

    fn signed_bundle(directory: &Path, download: &str) -> String {
        let bytes = package("org.test.bundled", "1.0.0", ">=2");
        fs::write(directory.join("plugin.zip"), &bytes).unwrap();
        let metadata = install::inspect_package(&bytes).unwrap().metadata;
        let catalog = Catalog {
            plugins: vec![CatalogEntry {
                id: metadata.id,
                name: metadata.name,
                description: metadata.description,
                localizations: metadata.localizations,
                version: metadata.version,
                app_versions: metadata.app_versions,
                protocol_version: metadata.protocol_version,
                icon: metadata.icon,
                icon_url: "https://example.com/icon.png".into(),
                downloads: [("windows-x86_64".into(), download.into())].into(),
                sha256: Some(catalog::sha256_of(&bytes)),
                homepage: None,
            }],
        };
        // Disposable fixture keys; never the application release key.
        let keys = cargo_packager::sign::generate_key(Some(String::new())).unwrap();
        let path = directory.join("unsigned-catalog.json");
        fs::write(&path, serde_json::to_vec(&catalog).unwrap()).unwrap();
        let signature = cargo_packager::sign::sign_file(
            &cargo_packager::sign::SigningConfig::new()
                .private_key(keys.sk)
                .password(""),
            &path,
        )
        .unwrap();
        let signature = fs::read_to_string(signature).unwrap();
        fs::write(
            directory.join(catalog::CATALOG_NAME),
            serde_json::to_vec(&serde_json::json!({"catalog": catalog, "signature": signature}))
                .unwrap(),
        )
        .unwrap();
        keys.pk
    }

    #[test]
    fn bundled_packages_require_the_catalog_key_digest_and_safe_local_filename() {
        let directory = tempfile::tempdir().unwrap();
        let key = signed_bundle(directory.path(), "https://example.com/plugin.zip");
        let verify =
            |key: &str| verified_bundled_plugins(directory.path(), key, "windows-x86_64", "2.5.0");
        assert_eq!(verify(&key).unwrap().len(), 1);
        let other = cargo_packager::sign::generate_key(Some(String::new())).unwrap();
        assert!(verify(&other.pk).is_err());
        fs::write(directory.path().join("plugin.zip"), b"tampered").unwrap();
        assert!(verify(&key).is_err());
        let key = signed_bundle(directory.path(), "../outside.zip");
        assert!(verify(&key).is_err());
        let key = signed_bundle(directory.path(), "../plugin.zip:stream");
        assert!(verify(&key).is_err());
    }

    #[test]
    fn bundled_installation_is_disabled_once_and_respects_user_removal() {
        let directory = tempfile::tempdir().unwrap();
        let key = signed_bundle(directory.path(), "plugin.zip");
        let source = PluginSource::Production {
            owner: "unused".into(),
            repository: "unused".into(),
            public_key: key,
            platform: "windows-x86_64".into(),
            bundled_directory: Some(directory.path().to_path_buf()),
            bundled_catalog_sha256: None,
        };
        let storage = tempfile::tempdir().unwrap();
        let mut manager = crate::PluginManager::new(storage.path().to_path_buf()).unwrap();
        let stop = AtomicBool::new(false);
        source
            .install_bundled(&mut manager, "2.5.0", &stop)
            .unwrap();
        assert_eq!(manager.list_plugins().len(), 1);
        assert_eq!(manager.list_plugins()[0].1, crate::PluginState::Discovered);
        let data = manager.data_root("org.test.bundled", true).unwrap();
        fs::write(data.join("business.json"), b"saved business data").unwrap();
        source
            .install_bundled(&mut manager, "2.5.0", &stop)
            .unwrap();
        assert_eq!(
            fs::read(data.join("business.json")).unwrap(),
            b"saved business data"
        );
        manager.uninstall("org.test.bundled").unwrap();
        source
            .install_bundled(&mut manager, "2.5.0", &stop)
            .unwrap();
        assert!(manager.installed_version("org.test.bundled").is_none());
    }

    #[test]
    fn local_bundle_requires_the_compiled_pin_and_rejects_modified_catalog_or_zip() {
        let directory = tempfile::tempdir().unwrap();
        signed_bundle(directory.path(), "plugin.zip");
        let path = directory.path().join(catalog::CATALOG_NAME);
        let signed: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let bytes = serde_json::to_vec(&signed["catalog"]).unwrap();
        fs::write(&path, &bytes).unwrap();
        let pin = catalog::sha256_of(&bytes);
        assert!(verified_bundled_plugins(directory.path(), "", "windows-x86_64", "2.5.0").is_err());
        let verify = || {
            verified_bundled_plugins_with_pin(
                directory.path(),
                "",
                "windows-x86_64",
                "2.5.0",
                Some(&pin),
            )
        };
        assert_eq!(verify().unwrap().len(), 1);
        let mut changed = bytes.clone();
        changed.push(b' ');
        fs::write(&path, changed).unwrap();
        assert!(verify().is_err());
        fs::write(&path, bytes).unwrap();
        fs::write(directory.path().join("plugin.zip"), b"replaced executable").unwrap();
        assert!(verify().is_err());
    }
    #[test]
    fn discovers_latest_per_id_without_installing_or_executing() {
        let directory = tempfile::tempdir().unwrap();
        let mut entries = Vec::new();
        for (filename, id, version, app) in [
            ("org.test.a-1.0.0.zip", "org.test.a", "1.0.0", ">=2"),
            ("org.test.a-1.1.0.zip", "org.test.a", "1.1.0", ">=2"),
            ("org.test.b-1.0.0.zip", "org.test.b", "1.0.0", ">=2"),
            ("org.test.a-9.0.0.zip", "org.test.a", "9.0.0", ">=9"),
        ] {
            let bytes = package(id, version, app);
            fs::write(directory.path().join(filename), &bytes).unwrap();
            if filename != "org.test.a-9.0.0.zip" {
                let manifest = install::inspect_package(&bytes).unwrap();
                entries.push(CatalogEntry {
                    id: manifest.metadata.id,
                    name: manifest.metadata.name,
                    description: manifest.metadata.description,
                    localizations: manifest.metadata.localizations,
                    version: manifest.metadata.version,
                    app_versions: manifest.metadata.app_versions,
                    protocol_version: manifest.metadata.protocol_version,
                    icon: manifest.metadata.icon,
                    icon_url: "https://example.com/a.png".into(),
                    downloads: [("macos-aarch64".into(), filename.to_string())].into(),
                    sha256: Some(catalog::sha256_of(&bytes)),
                    homepage: None,
                });
            }
        }
        fs::write(
            directory.path().join("catalog.json"),
            serde_json::to_vec(&Catalog { plugins: entries }).unwrap(),
        )
        .unwrap();
        let source = PluginSource::Development {
            catalog: directory.path().join("catalog.json"),
            packages: directory.path().to_owned(),
        };
        let offers = source.discover("2.3.1", &AtomicBool::new(false)).unwrap();
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[0].metadata.version, "1.1.0");
        assert_eq!(offers[1].metadata.id, "org.test.b");
        assert!(!directory.path().join(crate::PACKAGES_DIRECTORY).exists());
        assert!(offers[0].read(&AtomicBool::new(false)).is_ok());
        assert_eq!(offers[0].icon_url, "https://example.com/a.png");
        let mut replaced = package("org.test.a", "1.1.0", ">=2");
        // ZIP readers accept trailing data, so this changes the digest while
        // leaving the manifest identity the catalog validates.
        replaced.extend_from_slice(b"replaced");
        fs::write(directory.path().join("org.test.a-1.1.0.zip"), replaced).unwrap();
        assert!(
            offers[0].read(&AtomicBool::new(false)).is_err(),
            "a replaced local package needs a fresh catalog/digest"
        );
    }
    #[test]
    fn missing_directory_empty_catalog_and_version_ordering() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            PluginSource::Development {
                catalog: root.path().join("not-built/catalog.json"),
                packages: root.path().join("not-built"),
            }
            .discover("2.3.1", &AtomicBool::new(false))
            .unwrap()
            .is_empty()
        );
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "2.0.0"));
    }
}

#[cfg(test)]
mod production_tests {
    use super::*;
    #[test]
    fn production_candidates_choose_latest_per_plugin_not_single_global_release() {
        let entry = |id: &str, version: &str, app: &str| CatalogEntry {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            localizations: Default::default(),
            version: version.into(),
            app_versions: app.into(),
            protocol_version: 1,
            icon: bongocat_plugin_sdk::PluginIcon::Emoji("A".into()),
            icon_url: "https://example.com/a.png".into(),
            downloads: Default::default(),
            sha256: None,
            homepage: None,
        };
        let candidates = [
            ("org.test.a", "1.0.0", ">=2"),
            ("org.test.a", "1.1.0", ">=2"),
            ("org.test.b", "1.0.0", ">=2"),
            ("org.test.a", "9.0.0", ">=9"),
        ]
        .into_iter()
        .map(|(id, version, app)| {
            from_catalog(
                &entry(id, version, app),
                "https://example.com/package.zip".into(),
                "0".repeat(64),
            )
        })
        .collect();
        let offers = select_candidates(candidates, "2.3.1").unwrap();
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[0].metadata.version, "1.1.0");
        assert_eq!(offers[1].metadata.id, "org.test.b");
    }
}
