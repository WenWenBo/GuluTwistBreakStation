use anyhow::{Context, Result, bail, ensure};
use gulu_storage::UpdatePackage;
use semver::Version;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, mpsc},
};
use velopack::{
    locator::{self, LocationContext, VelopackLocatorConfig},
    sources::{FileSource, UpdateSource},
    *,
};

pub const REPOSITORY: &str = "https://github.com/WenWenBo/GuluTwistBreakStation";
pub const PACKAGE_ID: &str = "app.gulutwist.breakstation";
pub fn channel() -> &'static str {
    if cfg!(target_os = "windows") {
        "win-x64-stable"
    } else if cfg!(target_arch = "aarch64") {
        "osx-arm64-stable"
    } else {
        "osx-x64-stable"
    }
}

// Diagnostic source and cache overrides are inseparable from a separate data directory.
fn diagnostic_data() -> Option<PathBuf> {
    std::env::var_os("GULU_DIAGNOSTICS")?;
    std::env::var_os("GULU_DATA_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}
fn config() -> Result<VelopackLocatorConfig> {
    let l = locator::auto_locate_app_manifest(LocationContext::FromCurrentExe)?;
    let manifest = l.get_current_bin_dir().join("sq.version");
    let mut c = VelopackLocatorConfig {
        RootAppDir: l.get_root_dir(),
        ManifestPath: if manifest.exists() {
            manifest
        } else {
            l.get_root_dir().join("Contents/Resources/sq.version")
        },
        ..Default::default()
    };
    c.UpdateExePath = l.get_update_path();
    c.CurrentBinaryDir = l.get_current_bin_dir();
    c.IsPortable = l.get_is_portable();
    c.PackagesDir = diagnostic_data()
        .map(|p| p.join("velopack-packages"))
        .unwrap_or(l.get_packages_dir());
    Ok(c)
}
pub fn startup() {
    let app = VelopackApp::build().set_auto_apply_on_startup(false);
    #[cfg(target_os = "windows")]
    let app = app.set_app_user_model_id(PACKAGE_ID);
    let mut app = match config() {
        Ok(c) => app.set_locator(c),
        Err(_) => app,
    };
    app.run();
}

pub struct PreparationError {
    pub message: String,
    pub invalid_cache: bool,
}

pub struct Backend {
    manager: UpdateManager,
    packages: PathBuf,
    root: PathBuf,
    pub source: String,
}
impl Backend {
    pub fn new() -> Result<Self> {
        let c = config().context("当前为开发构建，自动更新不可用；请安装正式发行包")?;
        let manifest = c.load_manifest()?;
        ensure!(
            manifest.id == PACKAGE_ID && manifest.channel == channel(),
            "安装包渠道或身份不匹配"
        );
        ensure!(
            manifest.version == Version::parse(env!("CARGO_PKG_VERSION"))?,
            "安装包与程序版本不一致"
        );
        let local = diagnostic_data()
            .and_then(|_| std::env::var_os("GULU_UPDATE_DIR"))
            .map(PathBuf::from);
        let (source, provider): (String, Box<dyn UpdateSource>) = if let Some(path) = local {
            let path = path.canonicalize()?;
            (
                format!("file:{}", path.display()),
                Box::new(FileSource::new(path)),
            )
        } else {
            (REPOSITORY.into(), Box::new(Github::default()))
        };
        let packages = c.PackagesDir.clone();
        let root = c.RootAppDir.clone();
        let manager = UpdateManager::new_boxed(
            provider,
            Some(UpdateOptions {
                AllowVersionDowngrade: false,
                ExplicitChannel: Some(channel().into()),
                MaximumDeltasBeforeFallback: -1,
            }),
            Some(c),
        )?;
        Ok(Self {
            manager,
            packages,
            root,
            source,
        })
    }
    pub fn verify(&self, p: &UpdatePackage) -> Result<()> {
        p.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            p.source == self.source && p.channel == channel() && p.package_id == PACKAGE_ID,
            "缓存与当前更新源不匹配"
        );
        let version = Version::parse(&p.version)?;
        ensure!(
            version > self.manager.get_current_version() && version.pre.is_empty(),
            "缓存不是较新的正式版"
        );
        let path = self.packages.join(&p.file_name);
        verify_file(&path, p)?;
        let mut bundle = bundle::load_bundle_from_file(path)?;
        let manifest = bundle.read_manifest()?;
        ensure!(
            manifest.id == p.package_id
                && manifest.version == version
                && manifest.channel == p.channel
                && manifest.machine_architecture
                    == if cfg!(target_arch = "aarch64") {
                        "arm64"
                    } else {
                        "x64"
                    },
            "更新包内部身份与清单不匹配"
        );
        Ok(())
    }
    pub fn prepare(&self, p: &UpdatePackage) -> Result<(), PreparationError> {
        self.verify(p).map_err(|e| PreparationError {
            message: e.to_string(),
            invalid_cache: true,
        })?;
        self.check_install_location(p)
            .map_err(|e| PreparationError {
                message: e.to_string(),
                invalid_cache: false,
            })
    }
    fn check_install_location(&self, p: &UpdatePackage) -> Result<()> {
        let bundle = bundle::load_bundle_from_file(self.packages.join(&p.file_name))?;
        let (_, size) = bundle.calculate_size();
        let required = size.saturating_mul(2).saturating_add(50 * 1024 * 1024);
        ensure!(
            fs2::available_space(&self.packages)? > required
                && fs2::available_space(&self.root)? > required,
            "可用磁盘空间不足，请清理空间后重试"
        );
        let parent = self.root.parent().context("无法定位安装目录")?;
        let probe = parent.join(format!(".gulu-update-probe-{}", uuid::Uuid::new_v4()));
        let f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
            .context("安装目录不可写，请把应用移到可写的 Applications 目录")?;
        drop(f);
        std::fs::remove_file(probe)?;
        Ok(())
    }
    pub fn restore(&self, p: Option<UpdatePackage>) -> Result<Option<UpdatePackage>> {
        match p {
            Some(p) if Version::parse(&p.version)? <= self.manager.get_current_version() => {
                Ok(None)
            }
            Some(p) => {
                self.verify(&p)?;
                Ok(Some(p))
            }
            None => Ok(None),
        }
    }
    pub fn check(&self, progress: mpsc::Sender<i16>) -> Result<Option<UpdatePackage>> {
        let mut info = match self.manager.check_for_updates()? {
            UpdateCheck::UpdateAvailable(info) => info,
            UpdateCheck::NoUpdateAvailable => return Ok(None),
            UpdateCheck::RemoteIsEmpty => bail!("更新源没有本平台的正式发行包，请稍后重试"),
        };
        info.DeltasToTarget.clear();
        let a = &info.TargetFullRelease;
        let p = UpdatePackage {
            source: self.source.clone(),
            channel: channel().into(),
            package_id: a.PackageId.clone(),
            version: a.Version.clone(),
            file_name: a.FileName.clone(),
            size: a.Size,
            sha256: a.SHA256.clone(),
            notes: a.NotesMarkdown.chars().take(3000).collect(),
        };
        p.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            p.package_id == PACKAGE_ID
                && a.Type == "Full"
                && Version::parse(&p.version)?.pre.is_empty(),
            "清单包含不匹配的更新包"
        );
        std::fs::create_dir_all(&self.packages)?;
        ensure!(
            fs2::available_space(&self.packages)? > p.size.saturating_add(50 * 1024 * 1024),
            "下载空间不足"
        );
        let file = self.packages.join(&p.file_name);
        // The SDK skips validation for an already present filename. Never trust that shortcut.
        if file.exists() && verify_file(&file, &p).is_err() {
            std::fs::remove_file(&file)?;
        }
        let result = self.manager.download_updates(&info, Some(progress));
        if let Err(e) = result {
            bail!("下载失败：{e}");
        }
        self.verify(&p)?;
        Ok(Some(p))
    }
    // Called only after verified cache + successful final persistence and the last UI guard.
    pub fn arm(&self, p: &UpdatePackage) -> Result<()> {
        self.manager.wait_exit_then_apply_updates(
            VelopackAsset {
                PackageId: p.package_id.clone(),
                Version: p.version.clone(),
                Type: "Full".into(),
                FileName: p.file_name.clone(),
                SHA256: p.sha256.clone(),
                Size: p.size,
                ..Default::default()
            },
            false,
            true,
            Vec::<String>::new(),
        )?;
        Ok(())
    }
}

pub fn verify_file(path: &Path, p: &UpdatePackage) -> Result<()> {
    ensure!(
        !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
        "更新缓存不能是符号链接"
    );
    let mut f = std::fs::File::open(path)?;
    ensure!(f.metadata()?.len() == p.size, "更新包大小不符，请重新下载");
    let mut digest = Sha256::new();
    let mut buf = [0; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    ensure!(
        format!("{:x}", digest.finalize()).eq_ignore_ascii_case(&p.sha256),
        "更新包摘要不符，请重新下载"
    );
    Ok(())
}

// The native GithubSource uses unbounded HTTP calls and silently skips unreadable feeds.
// This narrow adapter retains Velopack's feed/download/package handling, with explicit
// timeouts, one immutable release per operation, and errors instead of false 'up to date'.
#[derive(Default)]
struct Github {
    downloads: Mutex<HashMap<String, String>>,
}
#[derive(serde::Deserialize)]
struct Release {
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}
fn options(timeout: u64) -> HttpOptions {
    HttpOptions {
        TimeoutMilliseconds: timeout,
        Headers: vec![HttpHeader {
            Name: "User-Agent".into(),
            Value: "GuluTwist-Updater".into(),
        }],
    }
}
impl UpdateSource for Github {
    fn get_release_feed(
        &self,
        channel: &str,
        _: &bundle::Manifest,
        _: &str,
    ) -> Result<VelopackAssetFeed, Error> {
        let json = download::download_url_as_string(
            "https://api.github.com/repos/WenWenBo/GuluTwistBreakStation/releases?per_page=10",
            Some(&options(30_000)),
        )?;
        let releases: Vec<Release> = serde_json::from_str(&json)?;
        let release = releases
            .into_iter()
            .find(|r| !r.draft && !r.prerelease)
            .ok_or_else(|| Error::Other("没有已公开的正式版本".into()))?;
        let name = format!("releases.{channel}.json");
        let url = release
            .assets
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| Error::Other("正式版缺少本平台清单".into()))?;
        let json =
            download::download_url_as_string(&url.browser_download_url, Some(&options(30_000)))?;
        let feed: VelopackAssetFeed = serde_json::from_str(&json)?;
        let mut downloads = self.downloads.lock().unwrap();
        downloads.clear();
        for asset in release.assets {
            if !asset
                .browser_download_url
                .starts_with("https://github.com/WenWenBo/GuluTwistBreakStation/releases/download/")
            {
                return Err(Error::Other("无效的 Release 附件地址".into()));
            }
            downloads.insert(asset.name, asset.browser_download_url);
        }
        Ok(feed)
    }
    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        path: &Path,
        progress: Option<mpsc::Sender<i16>>,
    ) -> Result<(), Error> {
        let url = self
            .downloads
            .lock()
            .unwrap()
            .get(&asset.FileName)
            .cloned()
            .ok_or_else(|| Error::Other("Release 缺少清单引用的全量包".into()))?;
        download::download_url_to_file(&url, path, Some(&options(30 * 60_000)), move |p| {
            if let Some(tx) = &progress {
                let _ = tx.send(p);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_package_is_rehashed_and_size_checked() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("test-full.nupkg");
        std::fs::write(&path, b"abc").unwrap();
        let p = UpdatePackage {
            source: REPOSITORY.into(),
            channel: channel().into(),
            package_id: PACKAGE_ID.into(),
            version: "9.0.0".into(),
            file_name: "test-full.nupkg".into(),
            size: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
            notes: String::new(),
        };
        assert!(verify_file(&path, &p).is_ok());
        std::fs::write(&path, b"abd").unwrap();
        assert!(verify_file(&path, &p).is_err());
        std::fs::write(&path, b"ab").unwrap();
        assert!(verify_file(&path, &p).is_err());
    }
}
