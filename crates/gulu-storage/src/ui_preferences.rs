use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPreferences {
    pub schema_version: u32,
    pub floating_enabled: bool,
    pub floating_position: Option<FloatingPosition>,
}
impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            floating_enabled: false,
            floating_position: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloatingPosition {
    pub display_uuid: Option<String>,
    pub x: f32,
    pub y: f32,
}
impl UiPreferences {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("悬浮窗设置来自不支持的版本");
        }
        if let Some(p) = &self.floating_position
            && (!p.x.is_finite()
                || !p.y.is_finite()
                || p.x.abs() > 100_000.
                || p.y.abs() > 100_000.)
        {
            return Err("悬浮窗位置无效");
        }
        Ok(())
    }
}
