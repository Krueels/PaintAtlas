//! Cached image masks shared by raster painting, previews and cursor outlines.
use image::{ImageReader, imageops::FilterType};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}
impl Mask {
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return 0.0;
        }
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let ix = x.floor() as isize;
        let iy = y.floor() as isize;
        let at = |x: isize, y: isize| {
            if x < 0 || y < 0 || x >= self.width as isize || y >= self.height as isize {
                0.0
            } else {
                self.alpha[y as usize * self.width + x as usize] as f32 / 255.0
            }
        };
        let a = x - x.floor();
        let b = y - y.floor();
        (at(ix, iy) * (1.0 - a) + at(ix + 1, iy) * a) * (1.0 - b)
            + (at(ix, iy + 1) * (1.0 - a) + at(ix + 1, iy + 1) * a) * b
    }
}
pub struct BrushTip {
    pub levels: Vec<Mask>,
    /// Closed contours in coordinates centered on the image, longest side = 1.
    pub contours: Vec<Vec<[f32; 2]>>,
}
impl BrushTip {
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut reader = ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        let img = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
        Self::from_rgba(img)
    }
    pub fn from_rgba(img: image::RgbaImage) -> Result<Self, String> {
        let translucent = img.pixels().any(|p| p[3] < 255);
        let mut gray = image::GrayImage::new(img.width(), img.height());
        for (out, p) in gray.pixels_mut().zip(img.pixels()) {
            out[0] = if translucent {
                p[3]
            } else {
                255 - ((p[0] as u32 * 54 + p[1] as u32 * 183 + p[2] as u32 * 19) / 256) as u8
            };
        }
        if !gray.pixels().any(|p| p[0] > 0) {
            return Err("The image does not contain a visible brush tip.".into());
        }
        let largest = gray.width().max(gray.height());
        if largest > 512 {
            gray = image::imageops::resize(
                &gray,
                (gray.width() * 512 / largest).max(1),
                (gray.height() * 512 / largest).max(1),
                FilterType::Triangle,
            );
        }
        let contour_max = gray.width().max(gray.height());
        let contour_mask = image::imageops::resize(
            &gray,
            (gray.width() * 96 / contour_max).clamp(1, 96),
            (gray.height() * 96 / contour_max).clamp(1, 96),
            FilterType::Triangle,
        );
        let mut contours = outline(&contour_mask);
        let max = gray.width().max(gray.height()) as f32;
        let contour_max = contour_mask.width().max(contour_mask.height()) as f32;
        let sx = (gray.width() as f32 / max) / (contour_mask.width() as f32 / contour_max);
        let sy = (gray.height() as f32 / max) / (contour_mask.height() as f32 / contour_max);
        for path in &mut contours {
            for p in path {
                p[0] *= sx;
                p[1] *= sy;
            }
        }
        let mut levels = Vec::new();
        loop {
            levels.push(Mask {
                width: gray.width() as usize,
                height: gray.height() as usize,
                alpha: gray.as_raw().clone(),
            });
            if gray.width() == 1 && gray.height() == 1 {
                break;
            }
            gray = image::imageops::resize(
                &gray,
                (gray.width() / 2).max(1),
                (gray.height() / 2).max(1),
                FilterType::Triangle,
            );
        }
        Ok(Self { levels, contours })
    }
    pub fn mask(&self, size: f32) -> &Mask {
        let largest = self.levels[0].width.max(self.levels[0].height) as f32;
        let level = (largest / size.max(1.0)).log2().max(0.0).floor() as usize;
        &self.levels[level.min(self.levels.len() - 1)]
    }
    pub fn proportions(&self) -> [f32; 2] {
        let m = &self.levels[0];
        let max = m.width.max(m.height) as f32;
        [m.width as f32 / max, m.height as f32 / max]
    }
    pub fn thumbnail(&self) -> Result<eframe::egui::ColorImage, String> {
        let m = &self.levels[0];
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(m.alpha.len())
            .map_err(|e| format!("Could not allocate the brush-tip thumbnail: {e}"))?;
        for &a in &m.alpha {
            let c = 255 - a;
            pixels.push(eframe::egui::Color32::from_rgb(c, c, c));
        }
        Ok(eframe::egui::ColorImage::new([m.width, m.height], pixels))
    }
}
fn outline(mask: &image::GrayImage) -> Vec<Vec<[f32; 2]>> {
    let w = mask.width() as i32;
    let h = mask.height() as i32;
    let threshold = (mask.as_raw().iter().copied().max().unwrap_or(0) / 4).clamp(1, 24);
    let solid = |x: i32, y: i32| {
        x >= 0 && y >= 0 && x < w && y < h && mask.get_pixel(x as u32, y as u32)[0] >= threshold
    };
    let mut edges = BTreeMap::<(i32, i32), Vec<(i32, i32)>>::new();
    for y in 0..h {
        for x in 0..w {
            if solid(x, y) {
                for (nx, ny, a, b) in [
                    (x, y - 1, (x, y), (x + 1, y)),
                    (x + 1, y, (x + 1, y), (x + 1, y + 1)),
                    (x, y + 1, (x + 1, y + 1), (x, y + 1)),
                    (x - 1, y, (x, y + 1), (x, y)),
                ] {
                    if !solid(nx, ny) {
                        edges.entry(a).or_default().push(b);
                    }
                }
            }
        }
    }
    let max = w.max(h) as f32;
    let mut paths = Vec::new();
    while let Some((&start, _)) = edges.first_key_value() {
        let mut path = vec![start];
        let mut at = start;
        while let Some(nexts) = edges.get_mut(&at) {
            let next = nexts.pop().unwrap();
            if nexts.is_empty() {
                edges.remove(&at);
            }
            path.push(next);
            at = next;
            if at == start {
                break;
            }
        }
        // Merge collinear edges; tracing is cached and never done during a stroke.
        let mut simple: Vec<(i32, i32)> = Vec::new();
        for p in path {
            if simple.len() >= 2 {
                let a = simple[simple.len() - 2];
                let b = simple[simple.len() - 1];
                if (b.0 - a.0) * (p.1 - b.1) == (b.1 - a.1) * (p.0 - b.0) {
                    simple.pop();
                }
            }
            simple.push(p);
        }
        if simple.len() >= 4 {
            paths.push(
                simple
                    .into_iter()
                    .map(|(x, y)| {
                        [
                            (x as f32 - w as f32 * 0.5) / max,
                            (y as f32 - h as f32 * 0.5) / max,
                        ]
                    })
                    .collect(),
            );
        }
    }
    paths
}
pub struct Entry {
    pub id: String,
    pub name: String,
    path: PathBuf,
}
pub struct Library {
    pub entries: Vec<Entry>,
    cache: HashMap<String, Result<Arc<BrushTip>, String>>,
    thumbnails: HashMap<String, eframe::egui::TextureHandle>,
    user_dir: PathBuf,
}
impl Library {
    pub fn new() -> Self {
        let user_dir = crate::presets::settings_path()
            .map(|p| p.with_file_name("brush_tips"))
            .unwrap_or_else(|| std::env::temp_dir().join("paintatlas-test-tips"));
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        let mut roots = Vec::new();
        if let Some(dir) = exe_dir {
            for ancestor in dir.ancestors().take(4) {
                roots.push(ancestor.join("assets/default_brushes"));
                // Keep compatibility with older unpacked builds.
                roots.push(ancestor.join("default_brushes"));
            }
        }
        roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/default_brushes"));
        let mut library = Self {
            entries: Vec::new(),
            cache: HashMap::new(),
            thumbnails: HashMap::new(),
            user_dir,
        };
        if let Some(root) = roots.into_iter().find(|p| p.is_dir()) {
            library.scan(&root, "default");
        }
        library.scan(&library.user_dir.clone(), "user");
        library.entries.sort_by(|a, b| a.name.cmp(&b.name));
        library
    }
    fn scan(&mut self, root: &Path, prefix: &str) {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                    matches!(
                        s.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp"
                    )
                }) {
                    let file = entry.file_name().to_string_lossy().into_owned();
                    self.entries.push(Entry {
                        id: format!("{prefix}:{file}"),
                        name: path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .replace(['-', '_'], " "),
                        path,
                    });
                }
            }
        }
    }
    pub fn name(&self, id: &str) -> &str {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map_or("Brush tip unavailable", |e| e.name.as_str())
    }
    pub fn get(&mut self, id: &str) -> Result<Arc<BrushTip>, String> {
        if let Some(result) = self.cache.get(id) {
            return result.clone();
        }
        let path = self
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.path.clone())
            .ok_or("Brush-tip file not found.")?;
        let result = BrushTip::load(&path).map(Arc::new);
        self.cache.insert(id.into(), result.clone());
        result
    }
    pub fn thumbnail(
        &mut self,
        ctx: &eframe::egui::Context,
        id: &str,
    ) -> Result<eframe::egui::TextureId, String> {
        if !self.thumbnails.contains_key(id) {
            let tip = self.get(id)?;
            self.thumbnails.insert(
                id.into(),
                ctx.load_texture(
                    format!("tip:{id}"),
                    tip.thumbnail()?,
                    eframe::egui::TextureOptions::LINEAR,
                ),
            );
        }
        Ok(self.thumbnails[id].id())
    }
    pub fn import(&mut self, source: &Path) -> Result<String, String> {
        let tip = BrushTip::load(source)?;
        std::fs::create_dir_all(&self.user_dir).map_err(|e| e.to_string())?;
        let stem: String = source
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .take(60)
            .collect();
        let stem = if stem.is_empty() { "ponta" } else { &stem };
        let mut n = 1;
        let path = loop {
            let path = self.user_dir.join(format!("{stem}-{n}.png"));
            if !path.exists() {
                break path;
            }
            n += 1;
        };
        let mask = &tip.levels[0];
        let rgba_len = mask
            .alpha
            .len()
            .checked_mul(4)
            .ok_or("The brush tip is too large.")?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(rgba_len)
            .map_err(|e| format!("Could not allocate the brush tip: {e}"))?;
        for &a in &mask.alpha {
            rgba.extend_from_slice(&[0, 0, 0, a]);
        }
        image::save_buffer(
            &path,
            &rgba,
            mask.width as u32,
            mask.height as u32,
            image::ColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
        let id = format!("user:{}", path.file_name().unwrap().to_string_lossy());
        self.entries.push(Entry {
            id: id.clone(),
            name: format!("{stem} (imported)"),
            path,
        });
        self.cache.insert(id.clone(), Ok(Arc::new(tip)));
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alpha_masks_opaque_grayscale_and_aspect_are_preserved() {
        let mut rgba = image::RgbaImage::from_pixel(16, 8, image::Rgba([0, 0, 0, 255]));
        for y in 2..6 {
            for x in 6..10 {
                rgba.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
        let tip = BrushTip::from_rgba(rgba).unwrap();
        assert_eq!(tip.proportions(), [1.0, 0.5]);
        assert!(tip.levels[0].sample(0.5, 0.5) < 0.01);
        assert!(tip.levels[0].sample(0.2, 0.5) > 0.99);
        assert!(tip.contours.len() >= 2, "cursor must retain holes");
        for p in tip.contours.iter().flatten() {
            assert!(p[0].abs() <= 0.501 && p[1].abs() <= 0.251);
        }
        let mut opaque = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]));
        for y in 2..6 {
            for x in 2..6 {
                opaque.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        let tip = BrushTip::from_rgba(opaque).unwrap();
        assert_eq!(tip.levels[0].alpha[0], 0);
        assert_eq!(tip.levels[0].alpha[3 * 8 + 3], 255);
        assert!(tip.mask(1.0).width <= 2);
    }
    #[test]
    fn imported_tip_remains_available_without_original_image() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.png");
        image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 0, 0, 255]))
            .save(&source)
            .unwrap();
        let mut library = Library::new();
        library.user_dir = dir.path().join("library");
        let id = library.import(&source).unwrap();
        std::fs::remove_file(source).unwrap();
        let mut restored = Library::new();
        restored.entries.clear();
        restored.cache.clear();
        restored.scan(&library.user_dir, "user");
        assert!(restored.get(&id).is_ok());
    }
    #[test]
    fn bundled_pack_decodes_with_masks_and_cursor_contours() {
        let mut library = Library::new();
        let ids: Vec<_> = library
            .entries
            .iter()
            .filter(|e| e.id.starts_with("default:"))
            .map(|e| e.id.clone())
            .collect();
        assert!(!ids.is_empty());
        for id in ids {
            let tip = library.get(&id).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(!tip.contours.is_empty(), "{id} has no cursor contour");
            assert!(tip.levels[0].width <= 512 && tip.levels[0].height <= 512);
        }
    }
}
