//! Straight RGBA images on the system clipboard. Keep the native owner alive
//! for Linux clipboard requests from other applications.
use crate::{
    document::{Document, MAX_SIDE},
    selection::Mask,
};
#[cfg(not(test))]
use std::borrow::Cow;
const NO_IMAGE: &str = "O clipboard não contém uma imagem. No navegador, use “Copiar imagem”, não “Copiar endereço da imagem”.";

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub size: [usize; 2],
    pub pixels: Vec<u8>,
}
impl Image {
    pub fn validate(&self) -> Result<(), String> {
        if self.size.contains(&0) || self.size.iter().any(|&v| v > MAX_SIDE as usize) {
            return Err(format!(
                "A imagem precisa ter entre 1 e {MAX_SIDE} pixels em cada dimensão."
            ));
        }
        if self.size[0]
            .checked_mul(self.size[1])
            .and_then(|n| n.checked_mul(4))
            != Some(self.pixels.len())
        {
            return Err("Dados de imagem inválidos no clipboard.".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct SystemClipboard {
    #[cfg(not(test))]
    native: Option<arboard::Clipboard>,
    // Unit tests must never read or overwrite the user's desktop clipboard.
    #[cfg(test)]
    image: Option<Image>,
}
impl SystemClipboard {
    #[cfg(not(test))]
    fn native(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.native.is_none() {
            self.native = Some(
                arboard::Clipboard::new().map_err(|e| format!("Clipboard indisponível: {e}"))?,
            );
        }
        Ok(self.native.as_mut().unwrap())
    }
    pub fn read_image(&mut self) -> Result<Image, String> {
        #[cfg(test)]
        let image = self.image.clone().ok_or_else(|| NO_IMAGE.to_owned())?;
        #[cfg(not(test))]
        let image = {
            let data = self.native()?.get_image().map_err(|e| match e {
                arboard::Error::ContentNotAvailable => NO_IMAGE.to_owned(),
                _ => format!("Não foi possível ler a imagem do clipboard: {e}"),
            })?;
            Image {
                size: [data.width, data.height],
                pixels: data.bytes.into_owned(),
            }
        };
        image.validate()?;
        Ok(image)
    }
    pub fn write_image(&mut self, image: &Image) -> Result<(), String> {
        image.validate()?;
        #[cfg(test)]
        {
            self.image = Some(image.clone());
            Ok(())
        }
        #[cfg(not(test))]
        {
            self.native()?
                .set_image(arboard::ImageData {
                    width: image.size[0],
                    height: image.size[1],
                    bytes: Cow::Borrowed(&image.pixels),
                })
                .map_err(|e| format!("Não foi possível copiar a imagem: {e}"))
        }
    }
}

pub fn capture(doc: &Document, mask: Option<&Mask>, merged: bool) -> Result<Image, String> {
    if mask.is_some_and(|m| m.width != doc.width || m.height != doc.height) {
        return Err("A seleção não corresponde ao canvas atual.".into());
    }
    let bounds = mask.map_or(
        eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            eframe::egui::Vec2::new(doc.width as f32, doc.height as f32),
        ),
        |m| m.bounds,
    );
    let x0 = bounds.min.x as usize;
    let y0 = bounds.min.y as usize;
    let size = [bounds.width() as usize, bounds.height() as usize];
    let merged_pixels = merged.then(|| doc.composite());
    let layer = &doc.layers[doc.active];
    let source = merged_pixels.as_deref().unwrap_or(&layer.pixels);
    let opacity = if merged { 1.0 } else { layer.opacity };
    let mut pixels = vec![0; size[0] * size[1] * 4];
    let mut visible = false;
    for y in 0..size[1] {
        for x in 0..size[0] {
            let dx = x0 + x;
            let dy = y0 + y;
            let i = (dy * doc.width as usize + dx) * 4;
            let j = (y * size[0] + x) * 4;
            let coverage = mask.map_or(1.0, |m| m.coverage(dx, dy));
            let alpha = (source[i + 3] as f32 * coverage * opacity).round() as u8;
            if alpha > 0 {
                pixels[j..j + 3].copy_from_slice(&source[i..i + 3]);
                pixels[j + 3] = alpha;
                visible = true;
            }
        }
    }
    if !visible {
        return Err(
            "Não há pixels para copiar. Selecione a camada do desenho ou use Copiar mesclado."
                .into(),
        );
    }
    let image = Image { size, pixels };
    image.validate()?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_preserves_mask_holes_partial_alpha_and_layer_opacity() {
        let mut doc = Document::new(4, 4, false);
        for pixel in doc.layers[0].pixels.as_chunks_mut::<4>().0 {
            *pixel = [200, 20, 40, 128];
        }
        doc.layers[0].opacity = 0.5;
        let original = doc.layers[0].pixels.clone();
        let mut values = vec![0; 16];
        values[5] = 255;
        values[6] = 128;
        values[10] = 255;
        let mask = Mask::from_values(4, 4, values).unwrap();
        let image = capture(&doc, Some(&mask), false).unwrap();
        assert_eq!(image.size, [2, 2]);
        assert_eq!(
            image.pixels,
            vec![
                200, 20, 40, 64, 200, 20, 40, 32, 0, 0, 0, 0, 200, 20, 40, 64
            ]
        );
        assert_eq!(doc.layers[0].pixels, original);
    }
    #[test]
    fn merged_copy_matches_visible_composite_and_empty_images_are_reported() {
        let mut doc = Document::new(2, 2, true);
        doc.layers[1].pixels[..4].copy_from_slice(&[255, 0, 0, 128]);
        doc.layers[1].opacity = 0.5;
        assert_eq!(capture(&doc, None, true).unwrap().pixels, doc.composite());
        assert_eq!(capture(&doc, None, false).unwrap().pixels[3], 64);
        doc.layers[1].pixels.fill(0);
        assert!(capture(&doc, None, false).is_err());
        assert!(capture(&doc, None, true).is_ok());
    }
    #[test]
    fn invalid_clipboard_dimensions_and_missing_images_do_not_reuse_old_pixels() {
        assert!(
            Image {
                size: [1, 1],
                pixels: vec![0; 3]
            }
            .validate()
            .is_err()
        );
        assert!(
            Image {
                size: [0, 1],
                pixels: vec![]
            }
            .validate()
            .is_err()
        );
        assert!(
            Image {
                size: [usize::MAX, 1],
                pixels: vec![]
            }
            .validate()
            .is_err()
        );
        let mut clipboard = SystemClipboard::default();
        let image = Image {
            size: [1, 1],
            pixels: vec![255; 4],
        };
        clipboard.write_image(&image).unwrap();
        assert_eq!(clipboard.read_image().unwrap(), image);
        clipboard.image = None; // Another application replaced the clipboard with text.
        assert!(clipboard.read_image().is_err());
    }
}
