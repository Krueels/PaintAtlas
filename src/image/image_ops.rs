//! RGB image operations. Color adjustments preserve alpha; spatial filters define their own alpha policy.
use crate::document::{Document, MAX_SIDE};
use crate::selection::Mask;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc,
};

// Leave room for independent preview/gallery work while bounding peak worker
// concurrency on modest systems. Canceled jobs retain their slot until exit.
const MAX_IMAGE_JOBS: usize = 8;
static ACTIVE_IMAGE_JOBS: AtomicUsize = AtomicUsize::new(0);
static ACTIVE_IMAGE_JOB_BYTES: AtomicU64 = AtomicU64::new(0);

fn checked_image_job_memory_total(current: u64, added: u64) -> Option<u64> {
    current.checked_add(added)
}

pub fn active_image_job_memory_bytes() -> usize {
    usize::try_from(ACTIVE_IMAGE_JOB_BYTES.load(Ordering::Acquire)).unwrap_or(usize::MAX)
}

struct ImageJobPermit {
    estimated_bytes: u64,
}
impl ImageJobPermit {
    fn acquire(estimated_bytes: usize) -> Result<Self, String> {
        let estimated_bytes = u64::try_from(estimated_bytes)
            .map_err(|_| "Estimativa de memória da tarefa excede o limite suportado.".to_owned())?;
        let mut active = ACTIVE_IMAGE_JOBS.load(Ordering::Acquire);
        loop {
            if active >= MAX_IMAGE_JOBS {
                return Err("Limite de tarefas de imagem simultâneas atingido. Aguarde ou cancele uma tarefa em andamento.".into());
            }
            match ACTIVE_IMAGE_JOBS.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    let mut bytes = ACTIVE_IMAGE_JOB_BYTES.load(Ordering::Acquire);
                    loop {
                        let Some(next_bytes) =
                            checked_image_job_memory_total(bytes, estimated_bytes)
                        else {
                            ACTIVE_IMAGE_JOBS.fetch_sub(1, Ordering::AcqRel);
                            return Err(
                                "Estimativa de memória das tarefas excede o limite suportado."
                                    .into(),
                            );
                        };
                        match ACTIVE_IMAGE_JOB_BYTES.compare_exchange_weak(
                            bytes,
                            next_bytes,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        ) {
                            Ok(_) => return Ok(Self { estimated_bytes }),
                            Err(current) => bytes = current,
                        }
                    }
                }
                Err(current) => active = current,
            }
        }
    }
}
impl Drop for ImageJobPermit {
    fn drop(&mut self) {
        ACTIVE_IMAGE_JOB_BYTES.fetch_sub(self.estimated_bytes, Ordering::AcqRel);
        ACTIVE_IMAGE_JOBS.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Adjustment {
    Filter,
    BrightnessContrast,
    Levels,
    Curves,
    Exposure,
    Vibrance,
    HueSaturation,
    ColorBalance,
    BlackWhite,
    PhotoFilter,
    ChannelMixer,
    Invert,
    Posterize,
    Threshold,
    GradientMap,
    ShadowsHighlights,
    Desaturate,
    ReplaceColor,
    Equalize,
    AutoTone,
    AutoContrast,
    AutoColor,
}
impl Adjustment {
    pub const ALL: &'static [Self] = &[
        Self::BrightnessContrast,
        Self::Levels,
        Self::Curves,
        Self::Exposure,
        Self::Vibrance,
        Self::HueSaturation,
        Self::ColorBalance,
        Self::BlackWhite,
        Self::PhotoFilter,
        Self::ChannelMixer,
        Self::Invert,
        Self::Posterize,
        Self::Threshold,
        Self::GradientMap,
        Self::ShadowsHighlights,
        Self::Desaturate,
        Self::ReplaceColor,
        Self::Equalize,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Filter => "Filtro",
            Self::BrightnessContrast => "Brightness / Contrast",
            Self::Levels => "Níveis",
            Self::Curves => "Curves",
            Self::Exposure => "Exposição",
            Self::Vibrance => "Vibração",
            Self::HueSaturation => "Matiz / saturação",
            Self::ColorBalance => "Equilíbrio de cores",
            Self::BlackWhite => "Black & White",
            Self::PhotoFilter => "Photo Filter",
            Self::ChannelMixer => "Channel Mixer",
            Self::Invert => "Inverter",
            Self::Posterize => "Posterize",
            Self::Threshold => "Threshold",
            Self::GradientMap => "Mapa de degradê",
            Self::ShadowsHighlights => "Shadows / Highlights",
            Self::Desaturate => "Desaturate",
            Self::ReplaceColor => "Replace Color",
            Self::Equalize => "Equalize",
            Self::AutoTone => "Tom automático",
            Self::AutoContrast => "Contraste automático",
            Self::AutoColor => "Cor automática",
        }
    }
    pub fn immediate(self) -> bool {
        matches!(
            self,
            Self::Invert
                | Self::Desaturate
                | Self::Equalize
                | Self::AutoTone
                | Self::AutoContrast
                | Self::AutoColor
        )
    }
}
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub curve_points: Vec<[f32; 2]>,
    pub filter: Option<crate::filters::Settings>,
    pub kind: Adjustment,
    pub p: [f32; 12],
    pub colors: [[u8; 3]; 2],
    pub preserve_luminosity: bool,
}
impl Settings {
    pub fn new(kind: Adjustment) -> Self {
        let mut p = [0.0; 12];
        match kind {
            Adjustment::Levels => {
                p[..5].copy_from_slice(&[0.0, 255.0, 1.0, 0.0, 255.0]);
            }
            Adjustment::Curves => {
                p[..5].copy_from_slice(&[0.0, 0.25, 0.5, 0.75, 1.0]);
            }
            Adjustment::Exposure => p[2] = 1.0,
            Adjustment::BlackWhite => p[..3].copy_from_slice(&[29.9, 58.7, 11.4]),
            Adjustment::PhotoFilter => p[0] = 25.0,
            Adjustment::ChannelMixer => {
                p[0] = 100.0;
                p[4] = 100.0;
                p[8] = 100.0;
            }
            Adjustment::Posterize => p[0] = 4.0,
            Adjustment::Threshold => p[0] = 128.0,
            Adjustment::ReplaceColor => {
                p[0] = 25.0;
                p[1] = 100.0;
            }
            _ => {}
        }
        Self {
            curve_points: Vec::new(),
            filter: None,
            kind,
            p,
            colors: if kind == Adjustment::PhotoFilter {
                [[255, 170, 90], [255; 3]]
            } else {
                [[0; 3], [255; 3]]
            },
            preserve_luminosity: true,
        }
    }
    pub fn label(&self) -> &'static str {
        self.filter
            .as_ref()
            .map_or(self.kind.label(), |filter| filter.kind.label())
    }
    pub fn reset(&mut self) {
        if let Some(filter) = &mut self.filter {
            filter.reset();
        } else {
            *self = Self::new(self.kind);
        }
    }
}
fn lum(v: [f32; 3]) -> f32 {
    v[0] * 0.299 + v[1] * 0.587 + v[2] * 0.114
}
fn rgb_hsl(v: [f32; 3]) -> [f32; 3] {
    let max = v[0].max(v[1]).max(v[2]);
    let min = v[0].min(v[1]).min(v[2]);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-6 {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == v[0] {
        ((v[1] - v[2]) / d).rem_euclid(6.0)
    } else if max == v[1] {
        (v[2] - v[0]) / d + 2.0
    } else {
        (v[0] - v[1]) / d + 4.0
    } / 6.0;
    [h, s, l]
}
fn hsl_rgb([h, s, l]: [f32; 3]) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h = h.rem_euclid(1.0) * 6.0;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let v = match h as u8 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    v.map(|v| v + l - c * 0.5)
}
fn keep_lum(mut out: [f32; 3], original: [f32; 3]) -> [f32; 3] {
    let d = lum(original) - lum(out);
    for c in &mut out {
        *c = (*c + d).clamp(0.0, 1.0);
    }
    out
}
fn curve(p: &[f32; 12], x: f32) -> f32 {
    let t = x * 4.0;
    let i = (t as usize).min(3);
    let t = t - i as f32;
    let a = p[i];
    let b = p[i + 1];
    let m0 = if i == 0 { b - a } else { (b - p[i - 1]) * 0.5 };
    let m1 = if i == 3 { b - a } else { (p[i + 2] - a) * 0.5 };
    ((2.0 * t * t * t - 3.0 * t * t + 1.0) * a
        + (t * t * t - 2.0 * t * t + t) * m0
        + (-2.0 * t * t * t + 3.0 * t * t) * b
        + (t * t * t - t * t) * m1)
        .clamp(0.0, 1.0)
}
pub fn curve_value(settings: &Settings, x: f32) -> f32 {
    let points = &settings.curve_points;
    if points.len() < 2 {
        return curve(&settings.p, x);
    }
    let i = points
        .partition_point(|p| p[0] <= x)
        .saturating_sub(1)
        .min(points.len() - 2);
    let a = points[i];
    let b = points[i + 1];
    let width = (b[0] - a[0]).max(0.0001);
    let t = ((x - a[0]) / width).clamp(0.0, 1.0);
    let slope = |i: usize| {
        (points[i + 1][1] - points[i][1]) / (points[i + 1][0] - points[i][0]).max(0.0001)
    };
    // Monotone Hermite tangents prevent overshoot around a dragged control point.
    let tangent = |i: usize| {
        if i == 0 {
            return slope(0);
        }
        if i == points.len() - 1 {
            return slope(i - 1);
        }
        let left = slope(i - 1);
        let right = slope(i);
        if left * right <= 0.0 {
            0.0
        } else {
            2.0 * left * right / (left + right)
        }
    };
    ((2.0 * t * t * t - 3.0 * t * t + 1.0) * a[1]
        + (t * t * t - 2.0 * t * t + t) * width * tangent(i)
        + (-2.0 * t * t * t + 3.0 * t * t) * b[1]
        + (t * t * t - t * t) * width * tangent(i + 1))
    .clamp(0.0, 1.0)
}

pub struct Job<T> {
    receiver: mpsc::Receiver<Result<T, String>>,
    cancel: Arc<AtomicBool>,
}
impl<T: Send + 'static> Job<T> {
    pub fn start_with_estimate(
        ctx: eframe::egui::Context,
        estimated_bytes: usize,
        work: impl FnOnce(&AtomicBool) -> Result<T, String> + Send + 'static,
    ) -> Result<Self, String> {
        let permit = ImageJobPermit::acquire(estimated_bytes)?;
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::Builder::new()
            .name("image-operation".into())
            .spawn(move || {
                let _permit = permit;
                let result = work(&flag);
                if !flag.load(Ordering::Relaxed) {
                    let _ = sender.send(result);
                    ctx.request_repaint();
                }
            })
            .map_err(|e| format!("Não foi possível iniciar o processamento: {e}"))?;
        Ok(Self { receiver, cancel })
    }
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.receiver.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("O processamento da imagem foi interrompido.".into())),
        }
    }
}
impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn canceled(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::Relaxed) {
        Err("Operação cancelada.".into())
    } else {
        Ok(())
    }
}

pub fn adjust(
    source: &[u8],
    mask: Option<&Mask>,
    settings: &Settings,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    if settings.kind == Adjustment::Filter {
        return crate::filters::apply(
            source,
            mask,
            settings.filter.as_ref().ok_or("Filtro não configurado.")?,
            cancel,
        );
    }
    let kind = settings.kind;
    let p = settings.p;
    let stats = matches!(
        kind,
        Adjustment::AutoTone
            | Adjustment::AutoContrast
            | Adjustment::AutoColor
            | Adjustment::Equalize
    );
    let mut hist = [[0u64; 256]; 4];
    if stats {
        for (i, pixel) in source.as_chunks::<4>().0.iter().enumerate() {
            if i % 4096 == 0 {
                canceled(cancel)?;
            }
            let weight = pixel[3] as u64 * mask.map_or(255, |m| m.values[i]) as u64;
            for c in 0..3 {
                hist[c][pixel[c] as usize] += weight;
            }
            hist[3][(lum([pixel[0] as f32, pixel[1] as f32, pixel[2] as f32])).round() as usize] +=
                weight;
        }
    }
    let mut lut = [[0.0f32; 256]; 3];
    for (channel, table) in lut.iter_mut().enumerate() {
        let h = &hist[if kind == Adjustment::AutoContrast {
            3
        } else {
            channel
        }];
        let total: u64 = h.iter().sum();
        let mut cumulative = 0;
        let low = h
            .iter()
            .position(|&n| {
                cumulative += n;
                cumulative > total / 200
            })
            .unwrap_or(0);
        cumulative = 0;
        let high = h
            .iter()
            .rposition(|&n| {
                cumulative += n;
                cumulative > total / 200
            })
            .unwrap_or(255);
        let first = *h.iter().find(|&&n| n > 0).unwrap_or(&0);
        let mut cdf = 0u64;
        for (i, value) in table.iter_mut().enumerate() {
            cdf += h[i];
            let x = i as f32 / 255.0;
            *value = match kind {
                Adjustment::Levels => {
                    let v = ((i as f32 - p[0]) / (p[1] - p[0]).max(1.0))
                        .clamp(0.0, 1.0)
                        .powf(1.0 / p[2].max(0.01));
                    (p[3] + v * (p[4] - p[3])) / 255.0
                }
                Adjustment::Curves if p[5] == 0.0 || p[5] as usize == channel + 1 => {
                    curve_value(settings, x)
                }
                Adjustment::AutoTone | Adjustment::AutoContrast | Adjustment::AutoColor
                    if high > low =>
                {
                    let v = ((i as f32 - low as f32) / (high - low) as f32).clamp(0.0, 1.0);
                    if kind == Adjustment::AutoColor {
                        // Stretch each channel, then gently neutralize its median.
                        let mut sum = 0u64;
                        let median = h
                            .iter()
                            .position(|&n| {
                                sum += n;
                                sum >= total / 2
                            })
                            .unwrap_or(128);
                        let mid =
                            ((median as f32 - low as f32) / (high - low) as f32).clamp(0.05, 0.95);
                        v.powf((0.5f32.ln() / mid.ln()).clamp(0.5, 2.0))
                    } else {
                        v
                    }
                }
                Adjustment::Equalize if total > first => {
                    (cdf.saturating_sub(first)) as f32 / (total - first) as f32
                }
                _ => x,
            };
        }
    }
    let mut out = Vec::new();
    out.try_reserve_exact(source.len())
        .map_err(|error| format!("Unable to allocate adjustment output: {error}"))?;
    out.extend_from_slice(source);
    for (i, (src, dst)) in source
        .as_chunks::<4>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<4>().0)
        .enumerate()
    {
        if i % 4096 == 0 {
            canceled(cancel)?;
        }
        let coverage = mask.map_or(1.0, |m| m.values[i] as f32 / 255.0);
        if src[3] == 0 || coverage == 0.0 {
            continue;
        }
        let v = [
            src[0] as f32 / 255.0,
            src[1] as f32 / 255.0,
            src[2] as f32 / 255.0,
        ];
        let l = lum(v);
        let rgb = match kind {
            Adjustment::Filter => {
                unreachable!("Filter processing is dispatched before color adjustments")
            }
            Adjustment::BrightnessContrast => {
                let c = p[1] * 2.55;
                let factor = 259.0 * (c + 255.0) / (255.0 * (259.0 - c));
                v.map(|x| factor * (x - 0.5) + 0.5 + p[0] / 100.0)
            }
            Adjustment::Levels
            | Adjustment::Curves
            | Adjustment::AutoTone
            | Adjustment::AutoContrast
            | Adjustment::AutoColor
            | Adjustment::Equalize => std::array::from_fn(|c| lut[c][src[c] as usize]),
            Adjustment::Exposure => v.map(|x| {
                (x * 2.0f32.powf(p[0]) + p[1])
                    .max(0.0)
                    .powf(1.0 / p[2].max(0.01))
            }),
            Adjustment::Vibrance | Adjustment::HueSaturation => {
                let mut hsl = rgb_hsl(v);
                if kind == Adjustment::Vibrance {
                    hsl[1] =
                        (hsl[1] + p[0] / 100.0 * (1.0 - hsl[1]) * hsl[1] * 2.0).clamp(0.0, 1.0);
                    hsl[1] = (hsl[1] * (1.0 + p[1] / 100.0)).clamp(0.0, 1.0);
                } else {
                    hsl[0] += p[0] / 360.0;
                    hsl[1] = (hsl[1] * (1.0 + p[1] / 100.0)).clamp(0.0, 1.0);
                    hsl[2] = if p[2] >= 0.0 {
                        hsl[2] + (1.0 - hsl[2]) * p[2] / 100.0
                    } else {
                        hsl[2] * (1.0 + p[2] / 100.0)
                    };
                }
                hsl_rgb(hsl)
            }
            Adjustment::ColorBalance => {
                let shadow = (1.0 - l * 2.0).max(0.0);
                let highlight = (l * 2.0 - 1.0).max(0.0);
                let weight = match p[3] as u8 {
                    0 => shadow,
                    2 => highlight,
                    _ => 1.0 - shadow - highlight,
                };
                let out = std::array::from_fn(|c| v[c] + p[c] / 200.0 * weight);
                if settings.preserve_luminosity {
                    keep_lum(out, v)
                } else {
                    out
                }
            }
            Adjustment::BlackWhite => {
                [v.iter().zip(p).take(3).map(|(v, w)| v * w / 100.0).sum(); 3]
            }
            Adjustment::PhotoFilter => {
                let density = p[0] / 100.0;
                let out = std::array::from_fn(|c| {
                    v[c] * (1.0 - density + density * settings.colors[0][c] as f32 / 255.0)
                });
                if settings.preserve_luminosity {
                    keep_lum(out, v)
                } else {
                    out
                }
            }
            Adjustment::ChannelMixer => {
                std::array::from_fn(|c| (0..3).map(|k| v[k] * p[c * 3 + k] / 100.0).sum())
            }
            Adjustment::Invert => v.map(|x| 1.0 - x),
            Adjustment::Posterize => {
                let n = (p[0].round() - 1.0).max(1.0);
                v.map(|x| (x * n).round() / n)
            }
            Adjustment::Threshold => [if l * 255.0 >= p[0] { 1.0 } else { 0.0 }; 3],
            Adjustment::GradientMap => std::array::from_fn(|c| {
                (settings.colors[0][c] as f32 * (1.0 - l) + settings.colors[1][c] as f32 * l)
                    / 255.0
            }),
            Adjustment::ShadowsHighlights => v.map(|x| {
                x + p[0] / 100.0 * (1.0 - l).powi(2) * (1.0 - x) - p[1] / 100.0 * l.powi(2) * x
            }),
            Adjustment::Desaturate => [l; 3],
            Adjustment::ReplaceColor => {
                let distance = (0..3)
                    .map(|c| (v[c] - settings.colors[0][c] as f32 / 255.0).powi(2))
                    .sum::<f32>()
                    .sqrt()
                    / 3.0f32.sqrt();
                let t = (1.0 - distance / (p[0] / 100.0).max(0.001)).clamp(0.0, 1.0);
                let weight = t * t * (3.0 - 2.0 * t) * p[1] / 100.0;
                std::array::from_fn(|c| {
                    v[c] * (1.0 - weight) + settings.colors[1][c] as f32 / 255.0 * weight
                })
            }
        };
        for c in 0..3 {
            dst[c] = ((v[c] + (rgb[c].clamp(0.0, 1.0) - v[c]) * coverage) * 255.0).round() as u8;
        }
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug)]
pub enum Geometry {
    Resize {
        width: u32,
        height: u32,
        filter: image::imageops::FilterType,
    },
    Canvas {
        width: u32,
        height: u32,
        anchor: [u8; 2],
    },
    Crop([u32; 4]),
    Trim {
        corner: u8,
        tolerance: u8,
        sides: [bool; 4],
    },
    Rotate90,
    Rotate180,
    Rotate270,
    Rotate(f32),
    FlipHorizontal,
    FlipVertical,
}
fn dimensions(doc: &Document, w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err("O resultado deve ter entre 1 e 4096 px por lado.".into());
    }
    if w as u64 * h as u64 * 4 * doc.layers.len() as u64 > 256 * 1024 * 1024 {
        return Err("O resultado excede o limite de 256 MiB de pixels do documento.".into());
    }
    Ok(())
}
fn zeroed_rgba(width: u32, height: u32) -> Result<Vec<u8>, String> {
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "Image operation output is too large.".to_owned())?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(len)
        .map_err(|error| format!("Unable to allocate image operation output: {error}"))?;
    pixels.resize(len, 0);
    Ok(pixels)
}
fn pixel(source: &[u8], width: u32, x: i32, y: i32, height: u32) -> [f32; 4] {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return [0.0; 4];
    }
    let p = &source[((y as u32 * width + x as u32) * 4) as usize..][..4];
    let a = p[3] as f32 / 255.0;
    [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, a]
}
fn bilinear(source: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 4] {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let mut out = [0.0; 4];
    for (dx, dy, weight) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let p = pixel(source, width, ix + dx, iy + dy, height);
        for c in 0..4 {
            out[c] += p[c] * weight;
        }
    }
    if out[3] <= 1e-6 {
        return [0; 4];
    }
    [
        (out[0] / out[3]).round() as u8,
        (out[1] / out[3]).round() as u8,
        (out[2] / out[3]).round() as u8,
        (out[3] * 255.0).round() as u8,
    ]
}
pub(crate) fn resized(
    source: Vec<u8>,
    w: u32,
    h: u32,
    nw: u32,
    nh: u32,
    filter: image::imageops::FilterType,
) -> Result<Vec<u8>, String> {
    if filter == image::imageops::FilterType::Nearest {
        return Ok(image::imageops::resize(
            &image::RgbaImage::from_raw(w, h, source).unwrap(),
            nw,
            nh,
            filter,
        )
        .into_raw());
    }
    // Filter premultiplied float RGBA to avoid dark fringes at transparent edges.
    let float_len = (source.len() / 4)
        .checked_mul(4)
        .ok_or_else(|| "Resampling input is too large.".to_owned())?;
    let mut floats = Vec::new();
    floats
        .try_reserve_exact(float_len)
        .map_err(|error| format!("Unable to allocate resampling input: {error}"))?;
    for p in source.as_chunks::<4>().0 {
        // Leave headroom for ringing: image::resize clamps float channels
        // to 0..1, which would otherwise clip alpha before RGB.
        let a = p[3] as f32 / 255.0 * 0.25;
        floats.extend_from_slice(&[
            p[0] as f32 / 255.0 * a,
            p[1] as f32 / 255.0 * a,
            p[2] as f32 / 255.0 * a,
            a,
        ]);
    }
    // The resampler now owns a float representation. Drop the consumed RGBA
    // input before image::resize allocates its horizontal and output buffers.
    drop(source);
    let image = image::ImageBuffer::<image::Rgba<f32>, Vec<f32>>::from_raw(w, h, floats).unwrap();
    let resized = image::imageops::resize(&image, nw, nh, filter);
    let output_len = (nw as usize)
        .checked_mul(nh as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "Resampling output is too large.".to_owned())?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(output_len)
        .map_err(|error| format!("Unable to allocate resampling output: {error}"))?;
    for pixel in resized.pixels() {
        let p = pixel.0;
        let a = (p[3] * 4.0).clamp(0.0, 1.0);
        if a < 1e-6 {
            output.extend_from_slice(&[0; 4]);
        } else {
            // Cubic/Lanczos kernels can overshoot alpha. Unpremultiply
            // with the original value before clamping the output alpha.
            output.extend_from_slice(&[
                (p[0] / p[3] * 255.0).round() as u8,
                (p[1] / p[3] * 255.0).round() as u8,
                (p[2] / p[3] * 255.0).round() as u8,
                (a * 255.0).round() as u8,
            ]);
        }
    }
    Ok(output)
}
pub fn geometry(
    mut doc: Document,
    mut op: Geometry,
    cancel: &AtomicBool,
) -> Result<Document, String> {
    if let Geometry::Rotate(angle) = op {
        let degrees = angle.rem_euclid(360.0);
        op = if degrees < 0.0001 || degrees > 359.9999 {
            Geometry::Canvas {
                width: doc.width,
                height: doc.height,
                anchor: [1, 1],
            }
        } else if (degrees - 90.0).abs() < 0.0001 {
            Geometry::Rotate90
        } else if (degrees - 180.0).abs() < 0.0001 {
            Geometry::Rotate180
        } else if (degrees - 270.0).abs() < 0.0001 {
            Geometry::Rotate270
        } else {
            Geometry::Rotate(degrees)
        };
    }
    let (w, h) = (doc.width, doc.height);
    if let Geometry::Trim {
        corner,
        tolerance,
        sides,
    } = op
    {
        let composite = doc.composite();
        canceled(cancel)?;
        let reference = if corner == 2 {
            &composite[composite.len() - 4..]
        } else {
            &composite[..4]
        };
        let mut bounds = [w, h, 0, 0];
        for (i, pixel) in composite.as_chunks::<4>().0.iter().enumerate() {
            if i % 4096 == 0 {
                canceled(cancel)?;
            }
            let content = if corner == 0 {
                pixel[3] > tolerance
            } else {
                (0..4).any(|c| pixel[c].abs_diff(reference[c]) > tolerance)
            };
            if content {
                let x = i as u32 % w;
                let y = i as u32 / w;
                bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x + 1),
                    bounds[3].max(y + 1),
                ];
            }
        }
        if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
            return Err("Não há conteúdo diferente da borda para aparar.".into());
        }
        if !sides[0] {
            bounds[0] = 0;
        }
        if !sides[1] {
            bounds[1] = 0;
        }
        if !sides[2] {
            bounds[2] = w;
        }
        if !sides[3] {
            bounds[3] = h;
        }
        // The full-canvas composite is only needed to find the crop bounds.
        // Release it before allocating transformed pixels for every layer.
        drop(composite);
        op = Geometry::Crop(bounds);
    }
    let (nw, nh) = match op {
        Geometry::Resize { width, height, .. } | Geometry::Canvas { width, height, .. } => {
            (width, height)
        }
        Geometry::Crop([x0, y0, x1, y1]) => {
            if x1 <= x0 || y1 <= y0 || x1 > w || y1 > h {
                return Err("Limites de recorte inválidos.".into());
            }
            (x1 - x0, y1 - y0)
        }
        Geometry::Rotate90 | Geometry::Rotate270 => (h, w),
        Geometry::Rotate(angle) => {
            let (s, c) = angle.to_radians().sin_cos();
            (
                (w as f32 * c.abs() + h as f32 * s.abs()).ceil() as u32,
                (w as f32 * s.abs() + h as f32 * c.abs()).ceil() as u32,
            )
        }
        _ => (w, h),
    };
    dimensions(&doc, nw, nh)?;
    let rotation = match op {
        Geometry::Rotate(angle) => Some(angle.to_radians().sin_cos()),
        _ => None,
    };
    for layer in &mut doc.layers {
        canceled(cancel)?;
        if let Geometry::Resize { filter, .. } = op {
            layer.pixels = resized(std::mem::take(&mut layer.pixels), w, h, nw, nh, filter)?;
            continue;
        }
        let source = &layer.pixels;
        let mut pixels = zeroed_rgba(nw, nh)?;
        for y in 0..nh {
            canceled(cancel)?;
            for x in 0..nw {
                let (sx, sy) = match op {
                    Geometry::Canvas { anchor, .. } => (
                        x as i32 - ((nw as i64 - w as i64) * anchor[0] as i64 / 2) as i32,
                        y as i32 - ((nh as i64 - h as i64) * anchor[1] as i64 / 2) as i32,
                    ),
                    Geometry::Crop([x0, y0, ..]) => ((x + x0) as i32, (y + y0) as i32),
                    Geometry::Rotate90 => (y as i32, h as i32 - 1 - x as i32),
                    Geometry::Rotate270 => (w as i32 - 1 - y as i32, x as i32),
                    Geometry::Rotate180 => (w as i32 - 1 - x as i32, h as i32 - 1 - y as i32),
                    Geometry::FlipHorizontal => (w as i32 - 1 - x as i32, y as i32),
                    Geometry::FlipVertical => (x as i32, h as i32 - 1 - y as i32),
                    Geometry::Rotate(_) => {
                        let (s, c) = rotation.expect("arbitrary rotation was precomputed");
                        let vx = x as f32 + 0.5 - nw as f32 / 2.0;
                        let vy = y as f32 + 0.5 - nh as f32 / 2.0;
                        let p = bilinear(
                            source,
                            w,
                            h,
                            c * vx + s * vy + w as f32 / 2.0 - 0.5,
                            -s * vx + c * vy + h as f32 / 2.0 - 0.5,
                        );
                        pixels[((y * nw + x) * 4) as usize..][..4].copy_from_slice(&p);
                        continue;
                    }
                    _ => unreachable!(),
                };
                if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                    let start = ((sy as u32 * w + sx as u32) * 4) as usize;
                    pixels[((y * nw + x) * 4) as usize..][..4]
                        .copy_from_slice(&source[start..start + 4]);
                }
            }
        }
        layer.pixels = pixels;
    }
    // Keep selection-restricted live group effects aligned with document geometry.
    for layer in &mut doc.layers {
        for effect in Arc::make_mut(&mut layer.effects) {
            if let Some(mask) = &effect.mask {
                let rgba_len = mask
                    .len()
                    .checked_mul(4)
                    .ok_or_else(|| "Effect mask is too large to transform.".to_owned())?;
                let mut mask_pixels = Vec::new();
                mask_pixels
                    .try_reserve_exact(rgba_len)
                    .map_err(|error| format!("Unable to allocate effect mask: {error}"))?;
                for &alpha in mask.iter() {
                    mask_pixels.extend_from_slice(&[255, 255, 255, alpha]);
                }
                let mask_doc = Document::try_from_rgba(w, h, mask_pixels)?;
                let transformed = geometry(mask_doc, op, cancel)?;
                let transformed_pixels = &transformed.layers[0].pixels;
                let mut transformed_mask = Vec::new();
                transformed_mask
                    .try_reserve_exact(transformed_pixels.len() / 4)
                    .map_err(|error| {
                        format!("Unable to allocate transformed effect mask: {error}")
                    })?;
                transformed_mask.extend(
                    transformed_pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|pixel| pixel[3]),
                );
                effect.mask = Some(Arc::new(transformed_mask));
            }
            effect.width = nw;
            effect.height = nh;
            if let Some(filter) = &mut effect.settings.filter {
                filter.width = nw as usize;
                filter.height = nh as usize;
            }
        }
    }
    doc.width = nw;
    doc.height = nh;
    doc.selection = None;
    doc.damage = None;
    doc.dirty_tiles.clear();
    *doc.stroke_tiles.get_mut() = None;
    canceled(cancel)?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_memory_estimate_aggregation_rejects_integer_overflow() {
        assert_eq!(checked_image_job_memory_total(10, 20), Some(30));
        assert_eq!(
            checked_image_job_memory_total(u64::MAX - 1, 1),
            Some(u64::MAX)
        );
        assert_eq!(checked_image_job_memory_total(u64::MAX, 1), None);
    }
    fn run(source: &[u8], settings: Settings) -> Vec<u8> {
        adjust(source, None, &settings, &AtomicBool::new(false)).unwrap()
    }
    fn geometry_run(doc: Document, op: Geometry) -> Document {
        geometry(doc, op, &AtomicBool::new(false)).unwrap()
    }
    #[test]
    fn geometry_output_allocation_reports_capacity_overflow() {
        assert!(zeroed_rgba(u32::MAX, u32::MAX).is_err());
        assert_eq!(zeroed_rgba(2, 2).unwrap(), vec![0; 16]);
    }
    fn grid() -> Document {
        let mut doc = Document::new(3, 2, false);
        doc.dpi = 600;
        doc.layers[0].name = "Original".into();
        doc.layers[0].label_color = Some([255, 100, 50]);
        doc.layers[0].pixels = (1..=6).flat_map(|v| [v, 0, 0, 255]).collect();
        doc
    }
    #[test]
    fn neutral_adjustments_are_identity_including_transparent_rgb() {
        let source = [
            20, 100, 210, 128, 0, 255, 0, 255, 255, 0, 255, 1, 12, 34, 56, 0,
        ];
        for kind in [
            Adjustment::BrightnessContrast,
            Adjustment::Levels,
            Adjustment::Curves,
            Adjustment::Exposure,
            Adjustment::Vibrance,
            Adjustment::HueSaturation,
            Adjustment::ColorBalance,
            Adjustment::ChannelMixer,
            Adjustment::ShadowsHighlights,
        ] {
            assert_eq!(run(&source, Settings::new(kind)), source, "{kind:?}");
        }
    }
    #[test]
    fn every_adjustment_preserves_alpha_and_unselected_pixels() {
        let source = [
            20, 100, 210, 128, 0, 255, 0, 255, 255, 0, 255, 1, 12, 34, 56, 0,
        ];
        let mask = Mask::from_values(4, 1, vec![255, 128, 0, 255]).unwrap();
        for kind in Adjustment::ALL.iter().chain(
            [
                Adjustment::AutoTone,
                Adjustment::AutoContrast,
                Adjustment::AutoColor,
            ]
            .iter(),
        ) {
            let out = adjust(
                &source,
                Some(&mask),
                &Settings::new(*kind),
                &AtomicBool::new(false),
            )
            .unwrap();
            for i in 0..4 {
                assert_eq!(out[i * 4 + 3], source[i * 4 + 3]);
            }
            assert_eq!(out[8..], source[8..]);
        }
    }
    #[test]
    fn invert_and_soft_selection_mix_rgb_without_changing_alpha() {
        let source = [10, 100, 200, 128, 10, 100, 200, 0];
        let mask = Mask::from_values(2, 1, vec![128, 255]).unwrap();
        let out = adjust(
            &source,
            Some(&mask),
            &Settings::new(Adjustment::Invert),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(out, [128, 128, 127, 128, 10, 100, 200, 0]);
        assert_eq!(
            run(&[10, 100, 200, 255], Settings::new(Adjustment::Invert)),
            [245, 155, 55, 255]
        );
    }
    #[test]
    fn levels_curves_exposure_and_channel_mixer_have_known_results() {
        let mut settings = Settings::new(Adjustment::Levels);
        settings.p[0] = 50.0;
        settings.p[1] = 200.0;
        assert_eq!(run(&[50, 125, 200, 255], settings), [0, 128, 255, 255]);
        let mut settings = Settings::new(Adjustment::Curves);
        settings.p[..5].copy_from_slice(&[1.0, 0.75, 0.5, 0.25, 0.0]);
        assert_eq!(run(&[0, 128, 255, 255], settings), [255, 127, 0, 255]);
        let mut settings = Settings::new(Adjustment::Exposure);
        settings.p[0] = 1.0;
        assert_eq!(run(&[32, 64, 128, 255], settings), [64, 128, 255, 255]);
        let mut settings = Settings::new(Adjustment::ChannelMixer);
        settings.p[..9].copy_from_slice(&[0.0, 100.0, 0.0, 100.0, 0.0, 0.0, 0.0, 0.0, 100.0]);
        assert_eq!(run(&[20, 100, 200, 255], settings), [100, 20, 200, 255]);
    }
    #[test]
    fn hue_desaturation_threshold_gradient_and_replace_color_are_functional() {
        let mut settings = Settings::new(Adjustment::HueSaturation);
        settings.p[0] = 120.0;
        assert_eq!(run(&[255, 0, 0, 255], settings), [0, 255, 0, 255]);
        assert_eq!(
            run(&[255, 0, 0, 255], Settings::new(Adjustment::Desaturate)),
            [76, 76, 76, 255]
        );
        assert_eq!(
            run(
                &[0, 0, 0, 255, 255, 255, 255, 128],
                Settings::new(Adjustment::Threshold)
            ),
            [0, 0, 0, 255, 255, 255, 255, 128]
        );
        let mut settings = Settings::new(Adjustment::GradientMap);
        settings.colors = [[255, 0, 0], [0, 0, 255]];
        assert_eq!(
            run(&[0, 0, 0, 255, 255, 255, 255, 128], settings),
            [255, 0, 0, 255, 0, 0, 255, 128]
        );
        let mut settings = Settings::new(Adjustment::ReplaceColor);
        settings.colors = [[255, 0, 0], [0, 255, 0]];
        settings.p[0] = 1.0;
        assert_eq!(
            run(&[255, 0, 0, 255, 0, 0, 255, 255], settings),
            [0, 255, 0, 255, 0, 0, 255, 255]
        );
    }
    #[test]
    fn automatic_histograms_ignore_transparent_and_unselected_pixels() {
        let source = [
            50, 50, 50, 255, 200, 200, 200, 255, 0, 0, 0, 0, 255, 255, 255, 255,
        ];
        let mask = Mask::from_values(4, 1, vec![255, 255, 255, 0]).unwrap();
        let out = adjust(
            &source,
            Some(&mask),
            &Settings::new(Adjustment::AutoContrast),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            out,
            [
                0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 0, 255, 255, 255, 255
            ]
        );
        for kind in [
            Adjustment::AutoTone,
            Adjustment::AutoContrast,
            Adjustment::AutoColor,
            Adjustment::Equalize,
        ] {
            let source = [100, 100, 100, 128];
            assert_eq!(run(&source, Settings::new(kind)), source);
        }
    }
    #[test]
    fn quarter_turns_and_flips_preserve_metadata_and_exact_pixels() {
        let original = grid();
        let rotated = geometry_run(original.clone(), Geometry::Rotate90);
        assert_eq!((rotated.width, rotated.height), (2, 3));
        assert_eq!(
            rotated.layers[0]
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            [4, 1, 5, 2, 6, 3]
        );
        assert_eq!(rotated.dpi, 600);
        assert_eq!(rotated.layers[0].label_color, Some([255, 100, 50]));
        assert_eq!(
            geometry_run(rotated, Geometry::Rotate270).layers[0].pixels,
            original.layers[0].pixels
        );
        for (angle, op) in [
            (0.0, Geometry::Rotate180),
            (90.0, Geometry::Rotate90),
            (-90.0, Geometry::Rotate270),
        ] {
            if angle != 0.0 {
                let a = geometry_run(original.clone(), Geometry::Rotate(angle));
                let b = geometry_run(original.clone(), op);
                assert_eq!((a.width, a.height), (b.width, b.height));
                assert_eq!(a.layers[0].pixels, b.layers[0].pixels);
            }
        }
        let flipped = geometry_run(
            geometry_run(original.clone(), Geometry::FlipHorizontal),
            Geometry::FlipVertical,
        );
        assert_eq!(
            flipped.layers[0].pixels,
            geometry_run(original, Geometry::Rotate180).layers[0].pixels
        );
    }
    #[test]
    fn canvas_anchor_crop_trim_and_all_layer_geometry_work() {
        let mut original = grid();
        original.add_layer();
        original.layers[1].pixels = original.layers[0].pixels.clone();
        let expanded = geometry_run(
            original.clone(),
            Geometry::Canvas {
                width: 5,
                height: 4,
                anchor: [1, 1],
            },
        );
        for layer in &expanded.layers {
            assert_eq!(layer.pixels[(5 + 1) * 4], 1);
            assert_eq!(layer.pixels[3], 0);
        }
        let cropped = geometry_run(expanded.clone(), Geometry::Crop([1, 1, 4, 3]));
        assert_eq!(cropped.layers[0].pixels, original.layers[0].pixels);
        assert_eq!(cropped.layers[1].pixels, original.layers[1].pixels);
        let trimmed = geometry_run(
            expanded,
            Geometry::Trim {
                corner: 0,
                tolerance: 0,
                sides: [true; 4],
            },
        );
        assert_eq!((trimmed.width, trimmed.height), (3, 2));
        assert_eq!(trimmed.layers[1].pixels, original.layers[1].pixels);
        let mut doc = Document::new(4, 4, false);
        doc.layers[0].pixels = [255; 4].repeat(16);
        doc.layers[0].pixels[(4 + 1) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
        let trim = geometry_run(
            doc,
            Geometry::Trim {
                corner: 1,
                tolerance: 0,
                sides: [true; 4],
            },
        );
        assert_eq!((trim.width, trim.height), (1, 1));
        assert_eq!(trim.layers[0].pixels, [255, 0, 0, 255]);
    }
    #[test]
    fn geometry_transforms_group_effect_masks_with_the_layer() {
        let mut doc = Document::new(3, 2, false);
        let mut mask = vec![0; 6];
        mask[1] = 64;
        mask[5] = 255;
        Arc::make_mut(&mut doc.layers[0].effects).push(crate::layer_groups::GroupEffect {
            settings: Settings::new(Adjustment::Invert),
            mask: Some(Arc::new(mask)),
            width: 3,
            height: 2,
        });

        let cropped = geometry_run(doc, Geometry::Crop([1, 0, 3, 2]));
        let effect = &cropped.layers[0].effects[0];
        assert_eq!((effect.width, effect.height), (2, 2));
        assert_eq!(effect.mask.as_deref().unwrap().as_slice(), &[64, 0, 0, 255]);
    }
    #[test]
    fn resampling_handles_pixel_art_and_transparent_edges() {
        let mut doc = Document::new(2, 1, false);
        doc.layers[0].pixels = vec![255, 0, 0, 255, 0, 0, 0, 0];
        for filter in [
            image::imageops::FilterType::Triangle,
            image::imageops::FilterType::CatmullRom,
            image::imageops::FilterType::Lanczos3,
        ] {
            let resized = geometry_run(
                doc.clone(),
                Geometry::Resize {
                    width: 8,
                    height: 1,
                    filter,
                },
            );
            for p in resized.layers[0].pixels.as_chunks::<4>().0 {
                if p[3] > 10 {
                    assert_eq!(&p[..3], &[255, 0, 0]);
                }
            }
        }
        let resized = geometry_run(
            doc,
            Geometry::Resize {
                width: 4,
                height: 1,
                filter: image::imageops::FilterType::Nearest,
            },
        );
        assert_eq!(
            resized.layers[0].pixels,
            [255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0]
        );
    }
    #[test]
    fn arbitrary_rotation_expands_bounds_and_checks_limits_before_allocation() {
        let doc = grid();
        let rotated = geometry_run(doc.clone(), Geometry::Rotate(45.0));
        assert!(rotated.width >= doc.width && rotated.height >= doc.height);
        assert!(
            rotated.layers[0]
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] > 0)
        );
        assert!(
            geometry(
                doc.clone(),
                Geometry::Resize {
                    width: 4097,
                    height: 2,
                    filter: image::imageops::FilterType::Nearest
                },
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(
            geometry(
                doc.clone(),
                Geometry::Crop([0, 0, 0, 1]),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        let mut many = doc;
        while many.layers.len() < 5 {
            many.add_layer();
        }
        assert!(
            geometry(
                many,
                Geometry::Canvas {
                    width: 4096,
                    height: 4096,
                    anchor: [1, 1]
                },
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    #[test]
    fn canceled_operations_do_not_return_partial_results() {
        assert!(
            adjust(
                &[100, 100, 100, 255],
                None,
                &Settings::new(Adjustment::Invert),
                &AtomicBool::new(true)
            )
            .is_err()
        );
        assert!(geometry(grid(), Geometry::Rotate90, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn canceled_or_rejected_geometry_keeps_the_caller_document_unchanged() {
        let original = grid();
        let pixels = original.layers[0].pixels.clone();
        let dims = (original.width, original.height);
        let result = geometry(original.clone(), Geometry::Rotate90, &AtomicBool::new(true));
        assert!(result.is_err());
        assert_eq!((original.width, original.height), dims);
        assert_eq!(original.layers[0].pixels, pixels);

        let result = geometry(
            original.clone(),
            Geometry::Crop([0, 0, 0, 1]),
            &AtomicBool::new(false),
        );
        assert!(result.is_err());
        assert_eq!((original.width, original.height), dims);
        assert_eq!(original.layers[0].pixels, pixels);
    }
    #[test]
    fn freely_positioned_curve_points_are_interpolated_and_used_by_the_raster_operation() {
        let mut settings = Settings::new(Adjustment::Curves);
        settings.curve_points = vec![[0.0, 0.0], [0.3, 0.7], [0.8, 0.9], [1.0, 1.0]];
        for point in &settings.curve_points {
            assert!((curve_value(&settings, point[0]) - point[1]).abs() < 0.0001);
        }
        let result = run(&[77, 204, 255, 200], settings.clone());
        assert!(result[0] > 178);
        assert_eq!(result[1], 230);
        assert_eq!(result[2], 255);
        assert_eq!(result[3], 200);
        settings.reset();
        assert!(settings.curve_points.is_empty());
        assert!((curve_value(&settings, 0.3) - 0.3).abs() < 0.0001);
    }
}
