// Mogged's window into the skin downloads: a player's own skin as pixels, for
// the sweep from or to it, and an HTTP GET for presets given as a URL. Added
// to `horizon_client::skins` as `mogged_access`.

/// A GET over HTTP or HTTPS, as the skin downloads make it.
pub fn http_get(url: &str) -> Option<Vec<u8>> {
    super::http_get(url)
}

impl super::Downloader {
    /// The 64x64 RGBA of the skin the atlas knows as `location`: a downloaded
    /// skin (in memory, else from the disk cache), the launcher's offline
    /// skin, or a default skin from the resource packs. None when it can't be had.
    pub fn mogged_pixels(&self, location: &str) -> Option<Vec<u8>> {
        if let Some(rgba) = self.images.get(location) {
            return Some(rgba.clone());
        }
        if let Some(digest) = location.strip_prefix("skins/") {
            let file = self
                .root
                .join(if digest.len() > 2 { &digest[..2] } else { "xx" })
                .join(digest);
            let (w, h, rgba) = crate::hud::decode_bytes(&std::fs::read(file).ok()?).ok()?;
            return super::process_legacy_skin(w, h, &rgba);
        }
        if location == "horizon/offline_skin" {
            return super::offline_skin().map(|(rgba, _)| rgba.clone());
        }
        // A default skin (`entity/player/wide/steve`).
        let file = self
            .resource_root
            .join(format!("assets/minecraft/textures/{location}.png"));
        let (w, h, rgba) = crate::hud::decode_bytes(&std::fs::read(file).ok()?).ok()?;
        super::resample_64(w, h, &rgba)
    }
}
