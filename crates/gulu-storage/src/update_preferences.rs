use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatePreferences {
    pub schema_version: u32,
    pub automatic: bool,
    pub last_attempt: i64,
    pub last_success: i64,
    pub retry_after: i64,
    pub ready: Option<UpdatePackage>,
}

impl Default for UpdatePreferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            automatic: true,
            last_attempt: 0,
            last_success: 0,
            retry_after: 0,
            ready: None,
        }
    }
}

/// The original feed metadata, retained for offline integrity checks (not a signature).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdatePackage {
    pub source: String,
    pub channel: String,
    pub package_id: String,
    pub version: String,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub notes: String,
}

impl UpdatePackage {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.file_name.is_empty()
            || self.file_name.contains(['/', '\\', ':'])
            || self.file_name.starts_with('.')
            || !self.file_name.ends_with("-full.nupkg")
            || self.size == 0
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || self.version.is_empty()
            || self.package_id.is_empty()
            || self.source.is_empty()
            || self.channel.is_empty()
        {
            return Err("更新包元数据无效，请重新检查更新");
        }
        Ok(())
    }
}

impl UpdatePreferences {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("更新设置来自不支持的版本，原数据已保留");
        }
        if let Some(package) = &self.ready {
            package.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_paths_and_original_digests_are_required() {
        let mut p = UpdatePackage {
            source: "github".into(),
            channel: "win-x64-stable".into(),
            package_id: "app".into(),
            version: "1.0.0".into(),
            file_name: "app-1.0.0-full.nupkg".into(),
            size: 1,
            sha256: "a".repeat(64),
            notes: String::new(),
        };
        assert!(p.validate().is_ok());
        for name in [
            "../app-full.nupkg",
            "a/b-full.nupkg",
            r"a\b-full.nupkg",
            "C:app-full.nupkg",
            "app-delta.nupkg",
        ] {
            p.file_name = name.into();
            assert!(p.validate().is_err());
        }
        p.file_name = "app-full.nupkg".into();
        p.sha256.clear();
        assert!(p.validate().is_err());
    }
}
