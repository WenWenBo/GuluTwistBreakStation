use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Artwork;
const IMAGES: &[(&str, &[u8])] = &[
    (
        "gulu/mascot.png",
        include_bytes!("../../../assets/icons/mascot.png"),
    ),
    (
        "gulu/app.png",
        include_bytes!("../../../assets/icons/app.png"),
    ),
];
impl AssetSource for Artwork {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        for (name, bytes) in IMAGES {
            if path == *name {
                return Ok(Some(Cow::Borrowed(bytes)));
            }
        }
        gpui_kit::assets::Assets.load(path)
    }
    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut items = gpui_kit::assets::Assets.list(path)?;
        items.extend(
            IMAGES
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::from(*name)),
        );
        Ok(items)
    }
}
