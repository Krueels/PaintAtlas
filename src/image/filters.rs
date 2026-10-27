//! CPU raster filters. Deterministic seeds, premultiplied sampling, cancellable rows.
use crate::selection::Mask;
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

macro_rules! catalog {
    ($( $kind:ident, $label:literal, $category:literal; )*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
        pub enum Kind { $($kind),* }
        impl Kind {
            pub const ALL: &'static [Self] = &[$(Self::$kind),*];
            pub fn label(self) -> &'static str {match self {$(Self::$kind => $label),*}}
            pub fn category(self) -> &'static str {match self {$(Self::$kind => $category),*}}
            pub fn shortcut(self) -> crate::shortcuts::Action {match self {$(Self::$kind => crate::shortcuts::Action::$kind),*}}
        }
    }
}
catalog! {
    Average, "Average", "Blur";
    Blur, "Blur", "Blur";
    BlurMore, "Blur More", "Blur";
    BoxBlur, "Box Blur", "Blur";
    GaussianBlur, "Gaussian Blur", "Blur";
    MotionBlur, "Motion Blur", "Blur";
    RadialBlur, "Radial Blur", "Blur";
    ZoomBlur, "Zoom Blur", "Blur";
    ShapeBlur, "Shape Blur", "Blur";
    SurfaceBlur, "Surface Blur", "Blur";
    IrisBlur, "Iris Blur", "Blur Gallery";
    TiltShift, "Tilt-Shift", "Blur Gallery";
    AddNoise, "Add Noise", "Noise";
    Despeckle, "Despeckle", "Noise";
    DustScratches, "Dust & Scratches", "Noise";
    Median, "Median", "Noise";
    ReduceNoise, "Reduce Noise", "Noise";
    Displace, "Displace", "Distort";
    Pinch, "Pinch", "Distort";
    Polar, "Polar Coordinates", "Distort";
    Ripple, "Ripple", "Distort";
    Shear, "Shear", "Distort";
    Spherize, "Spherize", "Distort";
    Twirl, "Twirl", "Distort";
    Wave, "Wave", "Distort";
    ZigZag, "ZigZag", "Distort";
    Glass, "Glass", "Distort";
    ColorHalftone, "Color Halftone", "Pixelate";
    Crystallize, "Crystallize", "Pixelate";
    Facet, "Facet", "Pixelate";
    Fragment, "Fragment", "Pixelate";
    Mezzotint, "Mezzotint", "Pixelate";
    Mosaic, "Mosaic", "Pixelate";
    Pointillize, "Pointillize", "Pixelate";
    Clouds, "Clouds", "Render";
    DifferenceClouds, "Difference Clouds", "Render";
    Fibers, "Fibers", "Render";
    LensFlare, "Lens Flare", "Render";
    Sharpen, "Sharpen", "Sharpen";
    SharpenMore, "Sharpen More", "Sharpen";
    SharpenEdges, "Sharpen Edges", "Sharpen";
    AdaptiveSharpen, "Adaptive Sharpen", "Sharpen";
    UnsharpMask, "Unsharp Mask", "Sharpen";
    Diffuse, "Diffuse", "Stylize";
    Emboss, "Emboss", "Stylize";
    FindEdges, "Edges", "Stylize";
    OilPaint, "Oil Paint", "Stylize";
    Solarize, "Solarize", "Stylize";
    TraceContour, "Trace Contour", "Stylize";
    Wind, "Wind", "Stylize";
    ColoredPencil, "Colored Pencil", "Artistic";
    DryBrush, "Dry Brush", "Artistic";
    FilmGrain, "Film Grain", "Artistic";
    NeonGlow, "Neon Glow", "Artistic";
    PosterEdges, "Poster Edges", "Artistic";
    Watercolor, "Watercolor", "Artistic";
    Crosshatch, "Crosshatch", "Brush Strokes";
    InkOutlines, "Ink Outlines", "Brush Strokes";
    GraphicPen, "Graphic Pen", "Sketch";
    Photocopy, "Photocopy", "Sketch";
    Stamp, "Stamp", "Sketch";
    HalftonePattern, "Halftone Pattern", "Sketch";
    Grain, "Grain", "Texture";
    MosaicTiles, "Mosaic Tiles", "Texture";
    Patchwork, "Patchwork", "Texture";
    StainedGlass, "Stained Glass", "Texture";
    Texturizer, "Texturizer", "Texture";
    Deinterlace, "De-Interlace", "Video";
    Custom, "Custom (3 × 3)", "Other";
    HighPass, "High Pass", "Other";
    Maximum, "Maximum", "Other";
    Minimum, "Minimum", "Other";
    Offset, "Offset", "Other";
}

pub const CATEGORIES: &[&str] = &[
    "Blur",
    "Blur Gallery",
    "Noise",
    "Distort",
    "Pixelate",
    "Render",
    "Sharpen",
    "Stylize",
    "Artistic",
    "Brush Strokes",
    "Sketch",
    "Texture",
    "Video",
    "Other",
];
#[derive(Clone, Copy)]
pub struct Control {
    pub index: usize,
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub integer: bool,
}
macro_rules! controls {($( $i:literal,$label:literal,$min:expr,$max:expr,$default:expr,$integer:literal);* $(;)?) => {&[$(Control {index:$i,label:$label,min:$min,max:$max,default:$default,integer:$integer}),*]};}
impl Kind {
    pub fn controls(self) -> &'static [Control] {
        use Kind::*;
        match self {
            Average | Blur | BlurMore | Facet | Despeckle | Sharpen | SharpenMore | Solarize
            | FindEdges => &[],
            BoxBlur | GaussianBlur | HighPass => controls![0, "Radius (px)", 0.0, 64.0, 4.0, false],
            MotionBlur => {
                controls![0,"Distance (px)",0.0,200.0,20.0,false;1,"Angle",-180.0,180.0,0.0,false;4,"Samples",4.0,32.0,12.0,true]
            }
            RadialBlur | ZoomBlur => {
                controls![0,"Strength",0.0,100.0,20.0,false;2,"Center X (%)",0.0,100.0,50.0,false;3,"Center Y (%)",0.0,100.0,50.0,false;4,"Samples",4.0,32.0,12.0,true]
            }
            ShapeBlur => {
                controls![0,"Radius (px)",0.0,64.0,8.0,false;1,"Shape: disc / hexagon / square",0.0,2.0,0.0,true]
            }
            SurfaceBlur | ReduceNoise => {
                controls![0,"Radius (px)",1.0,12.0,3.0,false;1,"Tolerance",1.0,100.0,25.0,false]
            }
            IrisBlur => {
                controls![0,"Blur (px)",0.0,64.0,8.0,false;1,"Focus area (%)",1.0,95.0,30.0,false;2,"Center X (%)",0.0,100.0,50.0,false;3,"Center Y (%)",0.0,100.0,50.0,false]
            }
            TiltShift => {
                controls![0,"Blur (px)",0.0,64.0,8.0,false;1,"Focus band (%)",1.0,95.0,25.0,false;2,"Position (%)",0.0,100.0,50.0,false;3,"Angle",-90.0,90.0,0.0,false]
            }
            AddNoise | FilmGrain | Grain => {
                controls![0,"Amount (%)",0.0,100.0,15.0,false;1,"Distribution: uniform / Gaussian",0.0,1.0,0.0,true;2,"Monochromatic",0.0,1.0,1.0,true]
            }
            Median => controls![0, "Radius (px)", 1.0, 12.0, 1.0, true],
            DustScratches => {
                controls![0,"Radius (px)",1.0,12.0,2.0,true;1,"Threshold",0.0,255.0,25.0,false]
            }
            Displace => {
                controls![0,"Offset X (px)",-200.0,200.0,20.0,false;1,"Offset Y (px)",-200.0,200.0,20.0,false]
            }
            Pinch | Spherize => {
                controls![0,"Strength (%)",-100.0,100.0,50.0,false;2,"Center X (%)",0.0,100.0,50.0,false;3,"Center Y (%)",0.0,100.0,50.0,false]
            }
            Polar => controls![
                0,
                "Direction: rectangular → polar / inverse",
                0.0,
                1.0,
                0.0,
                true
            ],
            Ripple | Wave => {
                controls![0,"Amplitude (px)",0.0,100.0,8.0,false;1,"Wavelength (px)",2.0,500.0,40.0,false;2,"Direction",-180.0,180.0,0.0,false]
            }
            Shear => {
                controls![0,"Deviation (px)",-300.0,300.0,40.0,false;1,"Curvature",-1.0,1.0,0.5,false]
            }
            Twirl => {
                controls![0,"Angle",-720.0,720.0,90.0,false;2,"Center X (%)",0.0,100.0,50.0,false;3,"Center Y (%)",0.0,100.0,50.0,false]
            }
            ZigZag => {
                controls![0,"Strength (px)",-50.0,50.0,8.0,false;1,"Ripples",1.0,20.0,5.0,false;2,"Center X (%)",0.0,100.0,50.0,false;3,"Center Y (%)",0.0,100.0,50.0,false]
            }
            Glass => {
                controls![0,"Distortion (px)",0.0,64.0,6.0,false;1,"Texture scale (px)",2.0,128.0,12.0,false]
            }
            ColorHalftone | HalftonePattern => {
                controls![0,"Size (px)",2.0,64.0,8.0,false;1,"Angle",-180.0,180.0,15.0,false]
            }
            Crystallize | Mosaic => controls![0, "Cell size (px)", 2.0, 128.0, 12.0, true],
            Pointillize | StainedGlass | Patchwork | MosaicTiles => {
                controls![0,"Cell size (px)",2.0,128.0,12.0,true;1,"Edge / gap (%)",0.0,50.0,10.0,false]
            }
            Fragment => controls![0, "Offset (px)", 0.0, 64.0, 4.0, false],
            Mezzotint => {
                controls![0,"Dot length (px)",1.0,16.0,1.0,true;1,"Color",0.0,1.0,0.0,true]
            }
            Clouds | DifferenceClouds => {
                controls![0,"Scale (px)",2.0,512.0,64.0,false;1,"Detail",1.0,6.0,4.0,true]
            }
            Fibers => {
                controls![0,"Thickness (px)",1.0,64.0,3.0,false;1,"Length (px)",2.0,512.0,100.0,false;2,"Contrast",0.1,5.0,1.5,false]
            }
            LensFlare => {
                controls![0,"Brightness (%)",0.0,200.0,75.0,false;1,"Center X (%)",0.0,100.0,35.0,false;2,"Center Y (%)",0.0,100.0,30.0,false]
            }
            SharpenEdges | UnsharpMask => {
                controls![0,"Strength (%)",0.0,300.0,100.0,false;1,"Radius (px)",0.1,32.0,2.0,false;2,"Threshold",0.0,100.0,5.0,false]
            }
            AdaptiveSharpen => {
                controls![0,"Strength (%)",0.0,300.0,100.0,false;1,"Radius (px)",0.1,32.0,2.0,false;2,"Threshold",0.0,100.0,5.0,false;3,"Reduce halos (%)",0.0,100.0,50.0,false]
            }
            Diffuse => controls![0, "Radius (px)", 0.0, 32.0, 2.0, false],
            Emboss => {
                controls![0,"Angle",-180.0,180.0,135.0,false;1,"Height (px)",1.0,16.0,1.0,false;2,"Strength (%)",0.0,300.0,100.0,false]
            }
            OilPaint | DryBrush | Watercolor => {
                controls![0,"Brush size (px)",1.0,12.0,3.0,false;1,"Color levels",2.0,64.0,12.0,true]
            }
            TraceContour => controls![0, "Level", 0.0, 255.0, 128.0, false],
            Wind => {
                controls![0,"Strength (px)",0.0,100.0,20.0,false;1,"Direction: left / right",0.0,1.0,0.0,true]
            }
            ColoredPencil => {
                controls![0,"Radius (px)",0.1,16.0,2.0,false;1,"Color (%)",0.0,100.0,60.0,false]
            }
            NeonGlow => {
                controls![0,"Strength",0.0,5.0,2.0,false;1,"Background brightness (%)",0.0,100.0,20.0,false]
            }
            InkOutlines => controls![0, "Edge strength", 0.0, 5.0, 2.0, false],
            PosterEdges => {
                controls![0,"Edge strength",0.0,5.0,2.0,false;1,"Color levels",2.0,64.0,8.0,true]
            }
            Crosshatch | GraphicPen => {
                controls![0,"Spacing (px)",2.0,24.0,5.0,false;1,"Contrast",0.1,3.0,1.0,false]
            }
            Photocopy | Stamp => {
                controls![0,"Threshold",0.0,255.0,128.0,false;1,"Detail",0.0,5.0,1.0,false]
            }
            Texturizer => {
                controls![0,"Weave scale (px)",2.0,64.0,8.0,false;1,"Depth (%)",0.0,100.0,25.0,false]
            }
            Deinterlace => controls![0, "Remove lines: even / odd", 0.0, 1.0, 0.0, true],
            Custom => {
                controls![9,"Divisor",-100.0,100.0,1.0,false;10,"Offset",-255.0,255.0,0.0,false]
            }
            Maximum | Minimum => controls![0, "Radius (px)", 1.0, 64.0, 1.0, true],
            Offset => {
                controls![0,"Horizontal (px)",-4096.0,4096.0,20.0,true;1,"Vertical (px)",-4096.0,4096.0,20.0,true]
            }
        }
    }
    pub fn approximation(self) -> bool {
        matches!(
            self,
            Self::GaussianBlur
                | Self::SurfaceBlur
                | Self::ReduceNoise
                | Self::AdaptiveSharpen
                | Self::OilPaint
                | Self::ColoredPencil
                | Self::DryBrush
                | Self::Watercolor
                | Self::Glass
                | Self::LensFlare
                | Self::IrisBlur
                | Self::TiltShift
        )
    }
    pub fn changes_alpha(self) -> bool {
        matches!(
            self,
            Self::Average
                | Self::Blur
                | Self::BlurMore
                | Self::BoxBlur
                | Self::GaussianBlur
                | Self::MotionBlur
                | Self::RadialBlur
                | Self::ZoomBlur
                | Self::ShapeBlur
                | Self::IrisBlur
                | Self::TiltShift
                | Self::Displace
                | Self::Pinch
                | Self::Polar
                | Self::Ripple
                | Self::Shear
                | Self::Spherize
                | Self::Twirl
                | Self::Wave
                | Self::ZigZag
                | Self::Glass
                | Self::Crystallize
                | Self::Fragment
                | Self::Mosaic
                | Self::Pointillize
                | Self::Clouds
                | Self::DifferenceClouds
                | Self::Fibers
                | Self::Diffuse
                | Self::Wind
                | Self::Offset
        )
    }
    pub fn seeded(self) -> bool {
        matches!(
            self,
            Self::AddNoise
                | Self::FilmGrain
                | Self::Grain
                | Self::Glass
                | Self::Crystallize
                | Self::Mezzotint
                | Self::Pointillize
                | Self::Clouds
                | Self::DifferenceClouds
                | Self::Fibers
                | Self::Diffuse
                | Self::StainedGlass
                | Self::GraphicPen
                | Self::Stamp
        )
    }
}
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Edge {
    Clamp,
    Wrap,
    Transparent,
}
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Map {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub kind: Kind,
    pub width: usize,
    pub height: usize,
    pub p: [f32; 12],
    pub seed: u32,
    pub colors: [[u8; 3]; 2],
    pub edge: Edge,
    pub mix: f32,
    pub alpha_lock: bool,
    pub preserve_alpha: bool,
    pub map: Option<Arc<Map>>,
}
impl Settings {
    pub fn new(kind: Kind, width: usize, height: usize) -> Self {
        let mut p = [0.0; 12];
        for control in kind.controls() {
            p[control.index] = control.default;
        }
        if kind == Kind::Custom {
            p[4] = 1.0;
        }
        Self {
            kind,
            width,
            height,
            p,
            seed: 12345,
            colors: if kind == Kind::NeonGlow {
                [[40, 180, 255], [255; 3]]
            } else {
                [[0; 3], [255; 3]]
            },
            edge: if kind == Kind::Offset {
                Edge::Wrap
            } else {
                Edge::Clamp
            },
            mix: 100.0,
            alpha_lock: false,
            preserve_alpha: false,
            map: None,
        }
    }
    pub fn reset(&mut self) {
        let old = self.clone();
        *self = Self::new(self.kind, self.width, self.height);
        self.colors = old.colors;
        self.alpha_lock = old.alpha_lock;
        self.preserve_alpha = false;
        self.map = old.map;
    }
}
fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Filter canceled.".into())
    } else {
        Ok(())
    }
}
fn hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846ca68b);
    value ^ (value >> 16)
}
fn random(x: i32, y: i32, seed: u32) -> f32 {
    hash((x as u32).wrapping_mul(0x9e3779b9) ^ (y as u32).wrapping_mul(0x85ebca6b) ^ seed) as f32
        / u32::MAX as f32
}
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let fx = fx * fx * (3.0 - 2.0 * fx);
    let fy = fy * fy * (3.0 - 2.0 * fy);
    let a = random(ix, iy, seed) * (1.0 - fx) + random(ix + 1, iy, seed) * fx;
    let b = random(ix, iy + 1, seed) * (1.0 - fx) + random(ix + 1, iy + 1, seed) * fx;
    a * (1.0 - fy) + b * fy
}
fn luma(v: [f32; 4]) -> f32 {
    v[0] * 0.299 + v[1] * 0.587 + v[2] * 0.114
}
fn rgba(p: &[u8]) -> [f32; 4] {
    [
        p[0] as f32 / 255.0,
        p[1] as f32 / 255.0,
        p[2] as f32 / 255.0,
        p[3] as f32 / 255.0,
    ]
}
fn encode(v: [f32; 4]) -> [u8; 4] {
    v.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}
struct Image<'a> {
    source: &'a [u8],
    w: usize,
    h: usize,
    edge: Edge,
}
impl Image<'_> {
    fn get(&self, mut x: i32, mut y: i32) -> [f32; 4] {
        match self.edge {
            Edge::Transparent if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 => {
                return [0.0; 4];
            }
            Edge::Wrap => {
                x = x.rem_euclid(self.w as i32);
                y = y.rem_euclid(self.h as i32);
            }
            _ => {
                x = x.clamp(0, self.w as i32 - 1);
                y = y.clamp(0, self.h as i32 - 1);
            }
        }
        rgba(&self.source[(y as usize * self.w + x as usize) * 4..][..4])
    }
    fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        let ix = x.floor() as i32;
        let iy = y.floor() as i32;
        let fx = x - x.floor();
        let fy = y - y.floor();
        let mut sum = [0.0; 4];
        for (dx, dy, weight) in [
            (0, 0, (1.0 - fx) * (1.0 - fy)),
            (1, 0, fx * (1.0 - fy)),
            (0, 1, (1.0 - fx) * fy),
            (1, 1, fx * fy),
        ] {
            let p = self.get(ix + dx, iy + dy);
            for c in 0..3 {
                sum[c] += p[c] * p[3] * weight;
            }
            sum[3] += p[3] * weight;
        }
        if sum[3] > 1e-6 {
            for c in 0..3 {
                sum[c] /= sum[3];
            }
        }
        sum
    }
    fn edge_strength(&self, x: i32, y: i32) -> f32 {
        let a = luma(self.get(x - 1, y - 1));
        let b = luma(self.get(x, y - 1));
        let c = luma(self.get(x + 1, y - 1));
        let d = luma(self.get(x - 1, y));
        let f = luma(self.get(x + 1, y));
        let g = luma(self.get(x - 1, y + 1));
        let h = luma(self.get(x, y + 1));
        let i = luma(self.get(x + 1, y + 1));
        ((c + 2.0 * f + i - a - 2.0 * d - g).powi(2) + (g + 2.0 * h + i - a - 2.0 * b - c).powi(2))
            .sqrt()
            * 0.25
    }
}
fn average_samples(samples: impl Iterator<Item = ([f32; 4], f32)>) -> [f32; 4] {
    let mut sum = [0.0; 4];
    let mut total = 0.0;
    for (p, weight) in samples {
        for c in 0..3 {
            sum[c] += p[c] * p[3] * weight;
        }
        sum[3] += p[3] * weight;
        total += weight;
    }
    if sum[3] > 1e-6 {
        for c in 0..3 {
            sum[c] /= sum[3];
        }
    }
    if total > 1e-6 {
        sum[3] /= total;
    }
    sum
}
// Sliding box passes on high precision premultiplied pixels: O(image area), not O(radius²).
fn box_pass(
    src: &[[u16; 4]],
    dst: &mut [[u16; 4]],
    w: usize,
    h: usize,
    r: usize,
    vertical: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4)
        .min(h);
    if workers <= 1 || w * h < 256 * 256 {
        return box_pass_rows(src, dst, [w, h, 0], r, vertical, cancel);
    }
    let rows = h.div_ceil(workers);
    std::thread::scope(|scope| {
        let mut tasks = Vec::new();
        tasks
            .try_reserve_exact(workers)
            .map_err(|error| format!("Unable to allocate blur workers: {error}"))?;
        for (index, output) in dst.chunks_mut(rows * w).enumerate() {
            tasks.push(scope.spawn(move || {
                box_pass_rows(src, output, [w, h, index * rows], r, vertical, cancel)
            }));
        }
        for task in tasks {
            task.join()
                .map_err(|_| "Parallel blur processing failed.".to_string())??;
        }
        Ok(())
    })
}
fn box_pass_rows(
    src: &[[u16; 4]],
    dst: &mut [[u16; 4]],
    dimensions: [usize; 3],
    r: usize,
    vertical: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let [w, h, first_row] = dimensions;
    let count = (r * 2 + 1) as u32;
    // Exact invariant division using a reciprocal and one remainder correction.
    // A channel sum fits in u32 even at the maximum supported radius.
    let reciprocal = (1_u64 << 32) / count as u64;
    let normalize = |sum: u32| {
        let value = sum + count / 2;
        let estimate = ((value as u64 * reciprocal) >> 32) as u32;
        (estimate + u32::from(value - estimate * count >= count)) as u16
    };
    if vertical {
        // Keep one running sum per column, but visit pixels row by row. Walking
        // whole columns on 3000px images caused cache misses on every read/write.
        let mut sums = Vec::new();
        sums.try_reserve_exact(w)
            .map_err(|error| format!("Unable to allocate blur row sums: {error}"))?;
        sums.resize(w, [0_u32; 4]);
        for offset in -(r as i32)..=r as i32 {
            check(cancel)?;
            let y = (first_row as i32 + offset).clamp(0, h as i32 - 1) as usize;
            for (sum, pixel) in sums.iter_mut().zip(&src[y * w..][..w]) {
                for c in 0..4 {
                    sum[c] += pixel[c] as u32;
                }
            }
        }
        for y in first_row..first_row + dst.len() / w {
            check(cancel)?;
            let output = &mut dst[(y - first_row) * w..][..w];
            let old = &src[y.saturating_sub(r) * w..][..w];
            let new = &src[(y + r + 1).min(h - 1) * w..][..w];
            for (((dst, sum), old), new) in output.iter_mut().zip(&mut sums).zip(old).zip(new) {
                *dst = sum.map(normalize);
                for c in 0..4 {
                    sum[c] = sum[c] - old[c] as u32 + new[c] as u32;
                }
            }
        }
    } else {
        let input = &src[first_row * w..][..dst.len()];
        for (input, output) in input.chunks_exact(w).zip(dst.chunks_exact_mut(w)) {
            check(cancel)?;
            let mut sum = input[0].map(|v| v as u32 * (r as u32 + 1));
            for x in 1..=r {
                for c in 0..4 {
                    sum[c] += input[x.min(w - 1)][c] as u32;
                }
            }
            for (x, dst) in output.iter_mut().enumerate() {
                *dst = sum.map(normalize);
                let old = input[x.saturating_sub(r)];
                let new = input[(x + r + 1).min(w - 1)];
                for c in 0..4 {
                    sum[c] = sum[c] - old[c] as u32 + new[c] as u32;
                }
            }
        }
    }

    Ok(())
}
fn map_pixels<S: Sync, D: Send>(
    source: &[S],
    destination: &mut [D],
    cancel: &AtomicBool,
    map: impl Fn(&S) -> D + Sync,
) -> Result<(), String> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4);
    if workers <= 1 || source.len() < 256 * 256 {
        for (i, (source, destination)) in source.iter().zip(destination).enumerate() {
            if i % 4096 == 0 {
                check(cancel)?;
            }
            *destination = map(source);
        }
        return Ok(());
    }
    let chunk = source.len().div_ceil(workers);
    std::thread::scope(|scope| {
        let mut tasks = Vec::new();
        let map = &map;
        for (source, destination) in source.chunks(chunk).zip(destination.chunks_mut(chunk)) {
            tasks.push(scope.spawn(move || {
                for (i, (source, destination)) in source.iter().zip(destination).enumerate() {
                    if i % 4096 == 0 {
                        check(cancel)?;
                    }
                    *destination = map(source);
                }
                Ok::<_, String>(())
            }));
        }
        for task in tasks {
            task.join()
                .map_err(|_| "Blur conversion failed.".to_string())??;
        }
        Ok(())
    })
}
fn blurred(
    source: &[u8],
    w: usize,
    h: usize,
    radius: f32,
    gaussian: bool,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    if radius <= 0.0 {
        return copied_buffer(source);
    }
    let r = if gaussian {
        (radius - 0.5).round().max(1.0) as usize
    } else {
        radius.round().max(1.0) as usize
    }
    .min(128);
    // Blank margins do not need six full-canvas passes. Include the complete
    // kernel support so cropping remains pixel-identical even at image edges.
    let mut bounds = [w, h, 0, 0];
    for (y, row) in source.chunks_exact(w * 4).enumerate() {
        check(cancel)?;
        let pixels = row.as_chunks::<4>().0;
        if let Some(left) = pixels.iter().position(|p| p[3] != 0) {
            let right = pixels.iter().rposition(|p| p[3] != 0).unwrap() + 1;
            bounds = [
                bounds[0].min(left),
                bounds[1].min(y),
                bounds[2].max(right),
                bounds[3].max(y + 1),
            ];
        }
    }
    if bounds[2] == 0 {
        return zeroed_buffer(source.len());
    }
    let margin = r * if gaussian { 3 } else { 1 };
    bounds = [
        bounds[0].saturating_sub(margin),
        bounds[1].saturating_sub(margin),
        (bounds[2] + margin).min(w),
        (bounds[3] + margin).min(h),
    ];
    let (cw, ch) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
    if cw * ch < w * h / 2 {
        let crop_len = cw
            .checked_mul(ch)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "Blur region is too large.".to_owned())?;
        let mut crop = Vec::new();
        crop.try_reserve_exact(crop_len)
            .map_err(|error| format!("Unable to allocate blur region: {error}"))?;
        for y in bounds[1]..bounds[3] {
            crop.extend_from_slice(&source[(y * w + bounds[0]) * 4..(y * w + bounds[2]) * 4]);
        }
        let filtered = blurred_full(&crop, cw, ch, r, gaussian, cancel)?;
        // The cropped input is no longer needed; release it before allocating
        // a full-size result buffer for the original canvas.
        drop(crop);
        let mut out = zeroed_buffer(source.len())?;
        for y in 0..ch {
            out[((bounds[1] + y) * w + bounds[0]) * 4..((bounds[1] + y) * w + bounds[2]) * 4]
                .copy_from_slice(&filtered[y * cw * 4..(y + 1) * cw * 4]);
        }
        return Ok(out);
    }
    blurred_full(source, w, h, r, gaussian, cancel)
}
fn zeroed_buffer(len: usize) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(len)
        .map_err(|error| format!("Unable to allocate filter buffer: {error}"))?;
    buffer.resize(len, 0);
    Ok(buffer)
}
fn copied_buffer(source: &[u8]) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(source.len())
        .map_err(|error| format!("Unable to allocate filter copy: {error}"))?;
    buffer.extend_from_slice(source);
    Ok(buffer)
}
fn zeroed_u16_pixels(len: usize) -> Result<Vec<[u16; 4]>, String> {
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(len)
        .map_err(|error| format!("Unable to allocate blur pixels: {error}"))?;
    pixels.resize(len, [0; 4]);
    Ok(pixels)
}
fn blurred_full(
    source: &[u8],
    w: usize,
    h: usize,
    r: usize,
    gaussian: bool,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let mut a = zeroed_u16_pixels(source.len() / 4)?;
    map_pixels(source.as_chunks::<4>().0, &mut a, cancel, |p| {
        [
            ((p[0] as u32 * p[3] as u32 * 257 + 127) / 255) as u16,
            ((p[1] as u32 * p[3] as u32 * 257 + 127) / 255) as u16,
            ((p[2] as u32 * p[3] as u32 * 257 + 127) / 255) as u16,
            p[3] as u16 * 257,
        ]
    })?;
    let mut b = zeroed_u16_pixels(a.len())?;
    for _ in 0..if gaussian { 3 } else { 1 } {
        box_pass(&a, &mut b, w, h, r, false, cancel)?;
        box_pass(&b, &mut a, w, h, r, true, cancel)?;
    }
    let mut out = zeroed_buffer(source.len())?;
    map_pixels(&a, out.as_chunks_mut::<4>().0, cancel, |p| {
        if p[3] == 0 {
            [0; 4]
        } else if p[3] == 65535 {
            // Opaque images need no unpremultiplication or floating point rounding.
            [
                ((p[0] as u32 + 128) / 257) as u8,
                ((p[1] as u32 + 128) / 257) as u8,
                ((p[2] as u32 + 128) / 257) as u8,
                255,
            ]
        } else {
            [
                (p[0] as f32 * 255.0 / p[3] as f32).round() as u8,
                (p[1] as f32 * 255.0 / p[3] as f32).round() as u8,
                (p[2] as f32 * 255.0 / p[3] as f32).round() as u8,
                ((p[3] as u32 + 128) / 257) as u8,
            ]
        }
    })?;

    Ok(out)
}
fn median(
    source: &[u8],
    w: usize,
    h: usize,
    r: usize,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let image = Image {
        source,
        w,
        h,
        edge: Edge::Clamp,
    };
    let mut out = copied_buffer(source)?;
    for y in 0..h {
        check(cancel)?;
        let mut hist = [[0i32; 256]; 3];
        let mut total = 0i32;
        let mut mids = [127usize; 3];
        let mut below = [0i32; 3];
        let update = |x: i32,
                      hist: &mut [[i32; 256]; 3],
                      total: &mut i32,
                      mids: &[usize; 3],
                      below: &mut [i32; 3],
                      sign: i32| {
            for dy in -(r as i32)..=r as i32 {
                let p = image.get(x, y as i32 + dy);
                let a = (p[3] * 255.0).round() as i32 * sign;
                *total += a;
                for c in 0..3 {
                    let v = (p[c] * 255.0).round() as usize;
                    hist[c][v] += a;
                    if v <= mids[c] {
                        below[c] += a;
                    }
                }
            }
        };
        for x in -(r as i32)..=r as i32 {
            update(x, &mut hist, &mut total, &mids, &mut below, 1);
        }
        for x in 0..w {
            let target = (total + 1) / 2;
            if total > 0 {
                for c in 0..3 {
                    while below[c] < target && mids[c] < 255 {
                        mids[c] += 1;
                        below[c] += hist[c][mids[c]];
                    }
                    while mids[c] > 0 && below[c] - hist[c][mids[c]] >= target {
                        below[c] -= hist[c][mids[c]];
                        mids[c] -= 1;
                    }
                    if source[(y * w + x) * 4 + 3] > 0 {
                        out[(y * w + x) * 4 + c] = mids[c] as u8;
                    }
                }
            }
            if x + 1 < w {
                update(
                    x as i32 - r as i32,
                    &mut hist,
                    &mut total,
                    &mids,
                    &mut below,
                    -1,
                );
                update(
                    x as i32 + r as i32 + 1,
                    &mut hist,
                    &mut total,
                    &mids,
                    &mut below,
                    1,
                );
            }
        }
    }
    Ok(out)
}
fn morphology(
    source: &[u8],
    w: usize,
    h: usize,
    r: usize,
    maximum: bool,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let mut a = copied_buffer(source)?;
    for pixel in a.as_chunks_mut::<4>().0 {
        if pixel[3] == 0 {
            pixel[..3].fill(if maximum { 0 } else { 255 });
        }
    }
    let mut b = copied_buffer(&a)?;
    for vertical in [false, true] {
        let (lines, len) = if vertical { (w, h) } else { (h, w) };
        let index = |line: usize, p: usize, c: usize| {
            if vertical {
                (p * w + line) * 4 + c
            } else {
                (line * w + p) * 4 + c
            }
        };
        for line in 0..lines {
            check(cancel)?;
            for c in 0..3 {
                let mut queue: VecDeque<(usize, u8)> = VecDeque::new();
                queue
                    .try_reserve(1)
                    .map_err(|error| format!("Unable to allocate morphology queue: {error}"))?;
                let mut next = 0;
                for p in 0..len {
                    while next <= (p + r).min(len - 1) {
                        let value = a[index(line, next, c)];
                        while queue
                            .back()
                            .is_some_and(|(_, v)| if maximum { *v <= value } else { *v >= value })
                        {
                            queue.pop_back();
                        }
                        if queue.len() == queue.capacity() {
                            queue.try_reserve(1).map_err(|error| {
                                format!("Unable to grow morphology queue: {error}")
                            })?;
                        }
                        queue.push_back((next, value));
                        next += 1;
                    }
                    while queue.front().is_some_and(|(i, _)| *i < p.saturating_sub(r)) {
                        queue.pop_front();
                    }
                    b[index(line, p, c)] = queue.front().unwrap().1;
                }
            }
        }
        std::mem::swap(&mut a, &mut b);
    }
    Ok(a)
}

fn mosaic(
    source: &[u8],
    w: usize,
    h: usize,
    size: usize,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let mut out = copied_buffer(source)?;
    for y0 in (0..h).step_by(size) {
        check(cancel)?;
        for x0 in (0..w).step_by(size) {
            let (x1, y1) = ((x0 + size).min(w), (y0 + size).min(h));
            let color =
                average_samples((y0..y1).flat_map(|y| {
                    (x0..x1).map(move |x| (rgba(&source[(y * w + x) * 4..][..4]), 1.0))
                }));
            let color = encode(color);
            for y in y0..y1 {
                for x in x0..x1 {
                    out[(y * w + x) * 4..][..4].copy_from_slice(&color);
                }
            }
        }
    }
    Ok(out)
}
fn paint(image: &Image<'_>, x: f32, y: f32, r: f32) -> [f32; 4] {
    let mut best = f32::INFINITY;
    let mut output = image.sample(x, y);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let mut sum = [0.0; 3];
        let mut count = 0.0;
        let mut square = 0.0;
        for dy in 0..3 {
            for dx in 0..3 {
                let p = image.sample(x + sx * dx as f32 * r / 2.0, y + sy * dy as f32 * r / 2.0);
                for c in 0..3 {
                    sum[c] += p[c] * p[3];
                }
                square += luma(p).powi(2) * p[3];
                count += p[3];
            }
        }
        if count > 1e-6 {
            let mean = sum.map(|v| v / count);
            let luminance = mean[0] * 0.299 + mean[1] * 0.587 + mean[2] * 0.114;
            let variance = square / count - luminance.powi(2);
            if variance < best {
                best = variance;
                output = [mean[0], mean[1], mean[2], output[3]];
            }
        }
    }
    output
}
fn range_blur(image: &Image<'_>, x: i32, y: i32, r: f32, tolerance: f32) -> [f32; 4] {
    let center = image.get(x, y);
    average_samples((-1..=1).flat_map(|dy| {
        (-1..=1).map(move |dx| {
            let p = image.sample(x as f32 + dx as f32 * r, y as f32 + dy as f32 * r);
            let distance = (0..3).map(|c| (p[c] - center[c]).powi(2)).sum::<f32>();
            let weight = (-distance / (2.0 * tolerance.max(0.005).powi(2))).exp()
                * if dx == 0 && dy == 0 { 4.0 } else { 1.0 };
            (p, weight)
        })
    }))
}
fn cell(image: &Image<'_>, x: f32, y: f32, size: f32, seed: u32) -> ([f32; 4], f32, f32) {
    let gx = (x / size).floor() as i32;
    let gy = (y / size).floor() as i32;
    let mut nearest = f32::INFINITY;
    let mut second = f32::INFINITY;
    let mut color = [0.0; 4];
    for dy in -1..=1 {
        for dx in -1..=1 {
            let cx = (gx + dx) as f32 * size + (0.15 + random(gx + dx, gy + dy, seed) * 0.7) * size;
            let cy = (gy + dy) as f32 * size
                + (0.15 + random(gx + dx, gy + dy, seed ^ 0xa531) * 0.7) * size;
            let distance = (x - cx).powi(2) + (y - cy).powi(2);
            if distance < nearest {
                second = nearest;
                nearest = distance;
                color = image.sample(cx, cy);
            } else if distance < second {
                second = distance;
            }
        }
    }
    (color, nearest.sqrt(), second.sqrt())
}
fn filtered(
    source: &[u8],
    mask: Option<&Mask>,
    settings: &Settings,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    use Kind::*;
    let Settings {
        kind,
        width: w,
        height: h,
        p,
        seed,
        edge,
        ..
    } = settings;
    let (w, h, kind, seed) = (*w, *h, *kind, *seed);
    let image = Image {
        source,
        w,
        h,
        edge: *edge,
    };
    let gaussian = match kind {
        Blur => Some((1.0, true)),
        BlurMore => Some((3.0, true)),
        BoxBlur => Some((p[0], false)),
        GaussianBlur | IrisBlur | TiltShift | HighPass | ColoredPencil => Some((p[0], true)),
        Sharpen | SharpenMore => Some((1.0, false)),
        SharpenEdges | AdaptiveSharpen | UnsharpMask => Some((p[1], true)),
        _ => None,
    };
    let prepared = if let Some((radius, gaussian)) = gaussian {
        Some(blurred(source, w, h, radius, gaussian, cancel)?)
    } else if matches!(kind, Median | DustScratches | Despeckle | DryBrush | Facet) {
        Some(median(
            source,
            w,
            h,
            if matches!(kind, Despeckle | Facet) {
                1
            } else {
                p[0].round().clamp(1.0, 12.0) as usize
            },
            cancel,
        )?)
    } else if matches!(kind, Mosaic | MosaicTiles | Patchwork) {
        Some(mosaic(
            source,
            w,
            h,
            p[0].round().max(2.0) as usize,
            cancel,
        )?)
    } else if matches!(kind, Maximum | Minimum) {
        Some(morphology(
            source,
            w,
            h,
            p[0].round().max(1.0) as usize,
            kind == Maximum,
            cancel,
        )?)
    } else {
        None
    };
    if matches!(kind, Blur | BlurMore | BoxBlur | GaussianBlur) {
        return Ok(prepared.unwrap());
    }
    let average = if kind == Average {
        average_samples(
            source
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .map(|(i, p)| (rgba(p), mask.map_or(1.0, |m| m.values[i] as f32 / 255.0))),
        )
    } else {
        [0.0; 4]
    };
    let mut out = copied_buffer(source)?;
    for y in 0..h {
        check(cancel)?;
        for x in 0..w {
            let i = (y * w + x) * 4;
            let original = rgba(&source[i..][..4]);
            if original[3] == 0.0 && !kind.changes_alpha() {
                continue;
            }
            let xf = x as f32;
            let yf = y as f32;
            let xi = x as i32;
            let yi = y as i32;
            let base = prepared
                .as_ref()
                .map_or(original, |pixels| rgba(&pixels[i..][..4]));
            let mut v = match kind {
                Average => average,
                Blur | BlurMore | BoxBlur | GaussianBlur | Median | Mosaic | Maximum | Minimum => {
                    base
                }
                IrisBlur | TiltShift => {
                    let distance = if kind == IrisBlur {
                        ((xf / w as f32 - p[2] / 100.0).powi(2)
                            + (yf / h as f32 - p[3] / 100.0).powi(2))
                        .sqrt()
                            * 2.0
                    } else {
                        let (s, c) = p[3].to_radians().sin_cos();
                        ((xf / w as f32 - 0.5) * s + (yf / h as f32 - p[2] / 100.0) * c).abs() * 2.0
                    };
                    let weight = ((distance - p[1] / 100.0) / (0.25f32)).clamp(0.0, 1.0);
                    let weight = weight * weight * (3.0 - 2.0 * weight);
                    average_samples([(original, 1.0 - weight), (base, weight)].into_iter())
                }
                SurfaceBlur | ReduceNoise => {
                    let mut value = range_blur(&image, xi, yi, p[0], p[1] / 100.0);
                    value[3] = original[3];
                    value
                }
                Despeckle => {
                    if image.edge_strength(xi, yi) < 0.15 {
                        base
                    } else {
                        original
                    }
                }
                DustScratches => {
                    if (0..3).any(|c| (base[c] - original[c]).abs() * 255.0 > p[1]) {
                        base
                    } else {
                        original
                    }
                }
                MotionBlur | RadialBlur | ZoomBlur | ShapeBlur => {
                    let count = if kind == ShapeBlur {
                        17
                    } else {
                        p[4].round().clamp(4.0, 32.0) as usize
                    };
                    let center = [p[2] / 100.0 * w as f32, p[3] / 100.0 * h as f32];
                    average_samples((0..count).map(|n| {
                        let t = n as f32 / (count - 1) as f32 - 0.5;
                        let (sx, sy) = match kind {
                            MotionBlur => {
                                let (s, c) = p[1].to_radians().sin_cos();
                                (xf + t * p[0] * c, yf + t * p[0] * s)
                            }
                            RadialBlur => {
                                let (s, c) = (t * p[0] * PI / 180.0).sin_cos();
                                let dx = xf - center[0];
                                let dy = yf - center[1];
                                (center[0] + c * dx - s * dy, center[1] + s * dx + c * dy)
                            }
                            ZoomBlur => {
                                let scale = 1.0 + t * p[0] / 100.0;
                                (
                                    center[0] + (xf - center[0]) * scale,
                                    center[1] + (yf - center[1]) * scale,
                                )
                            }
                            _ => {
                                if n == 0 {
                                    (xf, yf)
                                } else {
                                    let angle = n as f32 * 2.3999631;
                                    let r = (n as f32 / 16.0).sqrt() * p[0];
                                    let factor = match p[1] as u8 {
                                        1 => {
                                            (PI / 6.0).cos()
                                                / ((angle + PI / 6.0).rem_euclid(PI / 3.0)
                                                    - PI / 6.0)
                                                    .cos()
                                        }
                                        2 => {
                                            1.0 / angle.cos().abs().max(angle.sin().abs()).max(0.01)
                                        }
                                        _ => 1.0,
                                    };
                                    (xf + angle.cos() * r * factor, yf + angle.sin() * r * factor)
                                }
                            }
                        };
                        (image.sample(sx, sy), 1.0)
                    }))
                }
                Displace | Pinch | Polar | Ripple | Shear | Spherize | Twirl | Wave | ZigZag
                | Glass | Offset | Diffuse => {
                    let (sx, sy) = match kind {
                        Offset => (xf - p[0], yf - p[1]),
                        Displace => {
                            let map = if let Some(map) = &settings.map {
                                let mx = x * map.width / w;
                                let my = y * map.height / h;
                                rgba(&map.pixels[(my * map.width + mx) * 4..][..4])
                            } else {
                                original
                            };
                            (
                                xf + (map[0] - 0.5) * 2.0 * p[0],
                                yf + (map[1] - 0.5) * 2.0 * p[1],
                            )
                        }
                        Diffuse => (
                            xf + (random(xi, yi, seed) * 2.0 - 1.0) * p[0],
                            yf + (random(xi, yi, seed ^ 0x9351) * 2.0 - 1.0) * p[0],
                        ),
                        Glass => (
                            xf + (noise(xf / p[1].max(1.0), yf / p[1].max(1.0), seed) - 0.5)
                                * 2.0
                                * p[0],
                            yf + (noise(xf / p[1].max(1.0), yf / p[1].max(1.0), seed ^ 0xa531)
                                - 0.5)
                                * 2.0
                                * p[0],
                        ),
                        Ripple | Wave => {
                            let (s, c) = p[2].to_radians().sin_cos();
                            let along = xf * c + yf * s;
                            let offset = (along * TAU / p[1].max(2.0)).sin() * p[0];
                            if kind == Ripple {
                                (
                                    xf + offset,
                                    yf + (yf * TAU / p[1].max(2.0)).sin() * p[0] * 0.5,
                                )
                            } else {
                                (xf - s * offset, yf + c * offset)
                            }
                        }
                        Shear => {
                            let t = yf / h as f32 * 2.0 - 1.0;
                            (xf - p[0] * (t + p[1] * (1.0 - t * t)), yf)
                        }
                        Polar => {
                            let cx = (w - 1) as f32 / 2.0;
                            let cy = (h - 1) as f32 / 2.0;
                            let radius = w.min(h) as f32 / 2.0;
                            if p[0] < 0.5 {
                                let dx = xf - cx;
                                let dy = yf - cy;
                                (
                                    (dy.atan2(dx) / TAU + 0.5) * (w - 1) as f32,
                                    (dx * dx + dy * dy).sqrt() / radius * (h - 1) as f32,
                                )
                            } else {
                                let theta = (xf / (w - 1).max(1) as f32 - 0.5) * TAU;
                                let r = yf / (h - 1).max(1) as f32 * radius;
                                (cx + r * theta.cos(), cy + r * theta.sin())
                            }
                        }
                        _ => {
                            let cx = p[2] / 100.0 * w as f32;
                            let cy = p[3] / 100.0 * h as f32;
                            let rx = w as f32 / 2.0;
                            let ry = h as f32 / 2.0;
                            let dx = (xf - cx) / rx;
                            let dy = (yf - cy) / ry;
                            let r = (dx * dx + dy * dy).sqrt();
                            let theta = dy.atan2(dx);
                            if r >= 1.0 || r < 1e-6 {
                                (xf, yf)
                            } else {
                                let (r, angle) = match kind {
                                    Pinch => (r.powf((1.0 - p[0] / 125.0).max(0.2)), theta),
                                    Spherize => {
                                        let target = if p[0] >= 0.0 {
                                            r.asin() / (PI / 2.0)
                                        } else {
                                            (r * PI / 2.0).sin()
                                        };
                                        (r + (target - r) * p[0].abs() / 100.0, theta)
                                    }
                                    Twirl => (r, theta + p[0].to_radians() * (1.0 - r).powi(2)),
                                    _ => {
                                        (r + (r * p[1] * TAU).sin() * p[0] / rx * (1.0 - r), theta)
                                    }
                                };
                                (cx + r * angle.cos() * rx, cy + r * angle.sin() * ry)
                            }
                        }
                    };
                    image.sample(sx, sy)
                }
                Fragment => average_samples(
                    [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
                        .into_iter()
                        .map(|(dx, dy)| (image.sample(xf + dx * p[0], yf + dy * p[0]), 1.0)),
                ),
                Crystallize | Pointillize | StainedGlass => {
                    let size = p[0].max(2.0);
                    let (color, distance, second) = cell(&image, xf, yf, size, seed);
                    if kind == StainedGlass && second - distance < size * p[1] / 100.0 {
                        [
                            settings.colors[0][0] as f32 / 255.0,
                            settings.colors[0][1] as f32 / 255.0,
                            settings.colors[0][2] as f32 / 255.0,
                            original[3],
                        ]
                    } else if kind == Pointillize && distance > size * (0.5 - p[1] / 200.0) {
                        [
                            settings.colors[1][0] as f32 / 255.0,
                            settings.colors[1][1] as f32 / 255.0,
                            settings.colors[1][2] as f32 / 255.0,
                            color[3],
                        ]
                    } else {
                        color
                    }
                }
                Facet => {
                    let mut v = base;
                    for value in &mut v[..3] {
                        *value = (*value * 15.0).round() / 15.0;
                    }
                    v
                }
                MosaicTiles | Patchwork => {
                    let size = p[0].max(2.0);
                    let u = xf.rem_euclid(size) / size;
                    let v = yf.rem_euclid(size) / size;
                    let edge = u.min(1.0 - u).min(v.min(1.0 - v));
                    let mut color = base;
                    if kind == MosaicTiles && edge < p[1] / 200.0 {
                        for (c, value) in color[..3].iter_mut().enumerate() {
                            *value = settings.colors[1][c] as f32 / 255.0;
                        }
                    } else if kind == Patchwork {
                        let gap = p[1] / 100.0;
                        let shade = if u < gap || v < gap {
                            1.15
                        } else if u > 1.0 - gap || v > 1.0 - gap {
                            0.8
                        } else {
                            1.0
                        };
                        for value in &mut color[..3] {
                            *value *= shade;
                        }
                    }
                    color
                }
                ColorHalftone | HalftonePattern => {
                    let size = p[0].max(2.0);
                    let mut color = original;
                    for c in 0..3 {
                        let (s, co) = (p[1]
                            + if kind == ColorHalftone {
                                c as f32 * 30.0
                            } else {
                                0.0
                            })
                        .to_radians()
                        .sin_cos();
                        let dx = (xf * co + yf * s).rem_euclid(size) / size - 0.5;
                        let dy = (-xf * s + yf * co).rem_euclid(size) / size - 0.5;
                        let brightness = if kind == ColorHalftone {
                            original[c]
                        } else {
                            luma(original)
                        };
                        let radius = ((1.0 - brightness) / PI).sqrt();
                        let distance = (dx * dx + dy * dy).sqrt();
                        let ink = ((radius - distance) * size + 0.5).clamp(0.0, 1.0);
                        color[c] = 1.0 - ink;
                    }
                    color
                }
                Mezzotint => {
                    let mut color = original;
                    let gx = (xf / p[0].max(1.0)).floor() as i32;
                    for (c, value) in color[..3].iter_mut().enumerate() {
                        let threshold =
                            random(gx, yi, seed ^ if p[1] > 0.5 { c as u32 * 1093 } else { 0 });
                        let brightness = if p[1] > 0.5 {
                            original[c]
                        } else {
                            luma(original)
                        };
                        *value = if brightness > threshold { 1.0 } else { 0.0 };
                    }
                    color
                }
                AddNoise | FilmGrain | Grain => {
                    let mut color = original;
                    for (c, output) in color[..3].iter_mut().enumerate() {
                        let channel = if p[2] > 0.5 { 0 } else { c as u32 * 1301 };
                        let value = if p[1] > 0.5 {
                            (0..6)
                                .map(|n| random(xi, yi, seed ^ channel ^ (n * 491)))
                                .sum::<f32>()
                                - 3.0
                        } else {
                            random(xi, yi, seed ^ channel) * 2.0 - 1.0
                        };
                        let weight = if kind == FilmGrain {
                            (1.0 - luma(original) * 0.8).max(0.1)
                        } else if kind == Grain {
                            0.5 + noise(xf / 3.0, yf / 3.0, seed)
                        } else {
                            1.0
                        };
                        *output += value * p[0] / 100.0 * weight;
                    }
                    color
                }
                Clouds | DifferenceClouds | Fibers => {
                    let value = if kind == Fibers {
                        let n = noise(xf / p[0].max(1.0), yf / p[1].max(2.0), seed);
                        ((n - 0.5) * p[2] + 0.5).clamp(0.0, 1.0)
                    } else {
                        let mut sum = 0.0;
                        let mut weight = 1.0;
                        let mut total = 0.0;
                        for octave in 0..p[1].round().clamp(1.0, 6.0) as u32 {
                            let scale = 2.0f32.powi(octave as i32) / p[0].max(2.0);
                            sum += noise(xf * scale, yf * scale, seed ^ octave.wrapping_mul(7919))
                                * weight;
                            total += weight;
                            weight *= 0.5;
                        }
                        sum / total
                    };
                    let mut color = [0.0, 0.0, 0.0, 1.0];
                    for c in 0..3 {
                        let v = (settings.colors[0][c] as f32 * (1.0 - value)
                            + settings.colors[1][c] as f32 * value)
                            / 255.0;
                        color[c] = if kind == DifferenceClouds {
                            (original[c] - v).abs()
                        } else {
                            v
                        };
                    }
                    color
                }
                LensFlare => {
                    let cx = p[1] / 100.0 * w as f32;
                    let cy = p[2] / 100.0 * h as f32;
                    let dx = xf - cx;
                    let dy = yf - cy;
                    let radius = w.min(h) as f32;
                    let d = (dx * dx + dy * dy).sqrt() / radius;
                    let spot = (-d * d * 180.0).exp();
                    let angle = dy.atan2(dx);
                    let rays = angle.cos().abs().powi(30) + angle.sin().abs().powi(30);
                    let glow = (spot + rays * (-d * 12.0).exp() * 0.3) * p[0] / 100.0;
                    let ring = (-((d - 0.18) / 0.007).powi(2)).exp() * 0.12 * p[0] / 100.0;
                    let mut color = original;
                    for c in 0..3 {
                        color[c] = 1.0
                            - (1.0 - original[c])
                                * (1.0
                                    - (glow * [1.0, 0.8, 0.6][c] + ring * [0.5, 0.8, 1.0][c])
                                        .clamp(0.0, 1.0));
                    }
                    color
                }
                Sharpen | SharpenMore | SharpenEdges | AdaptiveSharpen | UnsharpMask | HighPass => {
                    let mut color = original;
                    let amount = match kind {
                        Sharpen => 0.8,
                        SharpenMore => 1.8,
                        _ => p[0] / 100.0,
                    };
                    let threshold = if matches!(kind, Sharpen | SharpenMore | HighPass) {
                        0.0
                    } else {
                        p[2] / 255.0
                    };
                    let edge_weight = if kind == SharpenEdges {
                        (image.edge_strength(xi, yi) * 4.0).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    for c in 0..3 {
                        let delta = original[c] - base[c];
                        if kind == HighPass {
                            color[c] = 0.5 + delta;
                        } else if delta.abs() >= threshold {
                            color[c] = original[c] + delta * amount * edge_weight;
                            if kind == AdaptiveSharpen {
                                let mut min = original[c];
                                let mut max = min;
                                for dy in -1..=1 {
                                    for dx in -1..=1 {
                                        let v = image.get(xi + dx, yi + dy)[c];
                                        min = min.min(v);
                                        max = max.max(v);
                                    }
                                }
                                let limited = color[c].clamp(min, max);
                                color[c] += (limited - color[c]) * p[3] / 100.0;
                            }
                        }
                    }
                    color
                }
                Emboss => {
                    let (s, c) = p[0].to_radians().sin_cos();
                    let a = luma(image.sample(xf + c * p[1], yf + s * p[1]));
                    let b = luma(image.sample(xf - c * p[1], yf - s * p[1]));
                    let value = 0.5 + (a - b) * p[2] / 100.0;
                    [value, value, value, original[3]]
                }
                FindEdges | InkOutlines | PosterEdges | NeonGlow => {
                    let edge = image.edge_strength(xi, yi);
                    let mut color = original;
                    for c in 0..3 {
                        color[c] = match kind {
                            FindEdges => 1.0 - edge,
                            InkOutlines => original[c] * (1.0 - edge * p[0]).max(0.0),
                            PosterEdges => {
                                let n = (p[1] - 1.0).max(1.0);
                                (original[c] * n).round() / n * (1.0 - edge * p[0]).max(0.0)
                            }
                            _ => {
                                original[c] * p[1] / 100.0
                                    + edge * p[0] * settings.colors[0][c] as f32 / 255.0
                            }
                        };
                    }
                    color
                }
                Solarize => {
                    let mut color = original;
                    for v in &mut color[..3] {
                        if *v > 0.5 {
                            *v = 1.0 - *v;
                        }
                    }
                    color
                }
                TraceContour => {
                    let center = luma(original) * 255.0 >= p[0];
                    let crossing = center != (luma(image.get(xi + 1, yi)) * 255.0 >= p[0])
                        || center != (luma(image.get(xi, yi + 1)) * 255.0 >= p[0]);
                    let value = if crossing { 0.0 } else { 1.0 };
                    [value, value, value, original[3]]
                }
                Wind => {
                    let sign = if p[1] > 0.5 { 1.0 } else { -1.0 };
                    let mut color = original;
                    for n in 1..=8 {
                        let v = image.sample(xf + sign * n as f32 * p[0] / 8.0, yf);
                        let weight = 1.0 - n as f32 / 10.0;
                        for c in 0..4 {
                            color[c] = color[c].max(v[c] * weight);
                        }
                    }
                    color
                }
                OilPaint | Watercolor => {
                    let mut color = paint(&image, xf, yf, p[0]);
                    let levels = (p[1] - 1.0).max(1.0);
                    let edge = if kind == Watercolor {
                        image.edge_strength(xi, yi) * 0.7
                    } else {
                        0.0
                    };
                    for value in &mut color[..3] {
                        *value = (*value * levels).round() / levels * (1.0 - edge).max(0.0);
                    }
                    color
                }
                DryBrush => {
                    let mut color = base;
                    let levels = (p[1] - 1.0).max(1.0);
                    for v in &mut color[..3] {
                        *v = (*v * levels).round() / levels;
                    }
                    color
                }
                ColoredPencil => {
                    let gray = luma(original);
                    let smooth = luma(base).max(0.001);
                    let pencil = (gray / smooth).clamp(0.0, 1.0);
                    let mut color = original;
                    for c in 0..3 {
                        color[c] = pencil * (1.0 - p[1] / 100.0 + original[c] * p[1] / 100.0);
                    }
                    color
                }
                Crosshatch | GraphicPen => {
                    let value = (luma(original) - 0.5) * p[1] + 0.5;
                    let size = p[0].max(2.0);
                    let first = (xf + yf).rem_euclid(size) < 1.0;
                    let second = (xf - yf).rem_euclid(size) < 1.0;
                    let ink = if kind == GraphicPen {
                        first && random(xi / 2, yi / 2, seed) > value
                    } else {
                        first && value < 0.75
                            || second && value < 0.4
                            || (xf.rem_euclid(size) < 1.0 && value < 0.2)
                    };
                    let t = if ink { 0.0 } else { 1.0 };
                    [t, t, t, original[3]]
                }
                Photocopy | Stamp => {
                    let v = if kind == Photocopy {
                        luma(original) - image.edge_strength(xi, yi) * p[1]
                    } else {
                        luma(original) + (noise(xf / 2.0, yf / 2.0, seed) - 0.5) * p[1] * 0.15
                    };
                    let t = if v * 255.0 >= p[0] { 1.0 } else { 0.0 };
                    [t, t, t, original[3]]
                }
                Texturizer => {
                    let u = xf * TAU / p[0].max(2.0);
                    let v = yf * TAU / p[0].max(2.0);
                    let shade = (u.sin() * 0.6 + v.sin() * 0.4) * p[1] / 100.0 * 0.5;
                    let mut color = original;
                    for value in &mut color[..3] {
                        *value += shade;
                    }
                    color
                }
                Deinterlace => {
                    if y % 2 == p[0] as usize {
                        average_samples(
                            [(image.get(xi, yi - 1), 1.0), (image.get(xi, yi + 1), 1.0)]
                                .into_iter(),
                        )
                    } else {
                        original
                    }
                }
                Custom => {
                    let divisor = if p[9].abs() < 1e-6 {
                        return Err("The convolution divisor cannot be zero.".into());
                    } else {
                        p[9]
                    };
                    let mut color = original;
                    for c in 0..3 {
                        let mut sum = 0.0;
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                let sample = image.get(xi + dx, yi + dy);
                                let value = if sample[3] > 0.0 {
                                    sample[c]
                                } else {
                                    original[c]
                                };
                                sum += value * p[((dy + 1) * 3 + dx + 1) as usize];
                            }
                        }
                        color[c] = sum / divisor + p[10] / 255.0;
                    }
                    color
                }
            };
            if !kind.changes_alpha() {
                v[3] = original[3];
            }
            out[i..][..4].copy_from_slice(&encode(v));
        }
    }
    Ok(out)
}

pub fn apply(
    source: &[u8],
    mask: Option<&Mask>,
    settings: &Settings,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    check(cancel)?;
    if settings.width == 0
        || settings.height == 0
        || settings.width > 4096
        || settings.height > 4096
        || settings
            .width
            .checked_mul(settings.height)
            .and_then(|v| v.checked_mul(4))
            != Some(source.len())
    {
        return Err("Invalid filter dimensions.".into());
    }
    if let Some(mask) = mask
        && (mask.width as usize != settings.width
            || mask.height as usize != settings.height
            || mask.values.len() != settings.width * settings.height)
    {
        return Err("The selection is incompatible with the filter.".into());
    }
    if let Some(map) = &settings.map
        && (map.width == 0
            || map.height == 0
            || map
                .width
                .checked_mul(map.height)
                .and_then(|v| v.checked_mul(4))
                != Some(map.pixels.len()))
    {
        return Err("Invalid displacement map.".into());
    }
    if !settings.p.iter().all(|v| v.is_finite()) || !settings.mix.is_finite() {
        return Err("Invalid filter parameters.".into());
    }
    if settings.mix <= 0.0 {
        return copied_buffer(source);
    }
    let mut settings = settings.clone();
    for c in settings.kind.controls() {
        settings.p[c.index] = settings.p[c.index].clamp(c.min, c.max);
    }
    let mut result = filtered(source, mask, &settings, cancel)?;
    let preserve = settings.alpha_lock || settings.preserve_alpha || !settings.kind.changes_alpha();
    if mask.is_none() && settings.mix >= 100.0 {
        if preserve {
            for (i, (dst, src)) in result
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(source.as_chunks::<4>().0)
                .enumerate()
            {
                if i % 4096 == 0 {
                    check(cancel)?;
                }
                if src[3] == 0 {
                    *dst = *src;
                } else {
                    dst[3] = src[3];
                }
            }
        }
        check(cancel)?;
        return Ok(result);
    }
    for (i, (dst, src)) in result
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(source.as_chunks::<4>().0)
        .enumerate()
    {
        if i % 4096 == 0 {
            check(cancel)?;
        }
        let m = mask.map_or(1.0, |m| m.values[i] as f32 / 255.0)
            * (settings.mix / 100.0).clamp(0.0, 1.0);
        if m <= 0.0 || preserve && src[3] == 0 {
            dst.copy_from_slice(src);
            continue;
        }
        if preserve {
            dst[3] = src[3];
        }
        if m >= 1.0 {
            continue;
        }
        let a = src[3] as f32 / 255.0;
        let b = dst[3] as f32 / 255.0;
        let alpha = a * (1.0 - m) + b * m;
        if alpha > 1e-6 {
            for c in 0..3 {
                dst[c] =
                    ((src[c] as f32 * a * (1.0 - m) + dst[c] as f32 * b * m) / alpha).round() as u8;
            }
            dst[3] = (alpha * 255.0).round() as u8;
        } else {
            dst.copy_from_slice(src);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pattern(w: usize, h: usize) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| {
                let x = i % w;
                let y = i / w;
                [
                    (x * 255 / (w - 1).max(1)) as u8,
                    (y * 255 / (h - 1).max(1)) as u8,
                    ((x * 17 + y * 31) % 256) as u8,
                    if (x + y).is_multiple_of(7) {
                        0
                    } else {
                        ((x + y) % 255 + 1) as u8
                    },
                ]
            })
            .collect()
    }
    fn run(source: &[u8], settings: &Settings) -> Vec<u8> {
        apply(source, None, settings, &AtomicBool::new(false)).unwrap()
    }
    #[test]
    fn fallible_filter_buffer_reports_capacity_overflow() {
        assert!(zeroed_buffer(usize::MAX).is_err());
        assert!(zeroed_u16_pixels(usize::MAX).is_err());
        assert_eq!(zeroed_buffer(16).unwrap(), vec![0; 16]);
        assert_eq!(copied_buffer(&[1, 2, 3]).unwrap(), vec![1, 2, 3]);
    }
    #[test]
    fn all_filters_preserve_dimensions_selection_alpha_lock_and_support_cancellation() {
        let (w, h) = (19, 13);
        let source = pattern(w, h);
        let mask = Mask::from_values(
            w as u32,
            h as u32,
            (0..w * h)
                .map(|i| {
                    if i % 3 == 0 {
                        0
                    } else if i % 3 == 1 {
                        128
                    } else {
                        255
                    }
                })
                .collect(),
        )
        .unwrap();
        for &kind in Kind::ALL {
            let mut settings = Settings::new(kind, w, h);
            settings.alpha_lock = true;
            let output = apply(&source, Some(&mask), &settings, &AtomicBool::new(false)).unwrap();
            assert_eq!(output.len(), source.len(), "{kind:?}");
            for (i, (src, dst)) in source
                .as_chunks::<4>()
                .0
                .iter()
                .zip(output.as_chunks::<4>().0)
                .enumerate()
            {
                assert_eq!(src[3], dst[3], "{kind:?}");
                if mask.values[i] == 0 || src[3] == 0 {
                    assert_eq!(src, dst, "{kind:?}");
                }
            }
            assert!(
                apply(&source, Some(&mask), &settings, &AtomicBool::new(true)).is_err(),
                "{kind:?}"
            );
            settings.mix = 0.0;
            assert_eq!(run(&source, &settings), source, "{kind:?}");
        }
    }
    #[test]
    fn gaussian_blur_spreads_alpha_without_dark_fringes_and_lock_preserves_shape() {
        let mut source = vec![0; 33 * 33 * 4];
        source[(16 * 33 + 16) * 4..][..4].copy_from_slice(&[255, 20, 0, 255]);
        let mut settings = Settings::new(Kind::GaussianBlur, 33, 33);
        settings.p[0] = 2.0;
        let output = run(&source, &settings);
        assert!(output[(16 * 33 + 15) * 4 + 3] > 0);
        assert!(output[(16 * 33 + 16) * 4 + 3] < 255);
        for pixel in output.as_chunks::<4>().0 {
            if pixel[3] > 0 {
                assert_eq!(pixel[0], 255);
                assert!(pixel[1].abs_diff(20) <= 1);
            }
        }
        settings.alpha_lock = true;
        let locked = run(&source, &settings);
        for (src, dst) in source
            .as_chunks::<4>()
            .0
            .iter()
            .zip(locked.as_chunks::<4>().0)
        {
            assert_eq!(src[3], dst[3]);
        }
        for kind in [Kind::BoxBlur, Kind::GaussianBlur] {
            for alpha in [1, 17, 128, 255] {
                let source = [200, 100, 50, alpha].repeat(5 * 7);
                let settings = Settings::new(kind, 5, 7);
                for p in run(&source, &settings).as_chunks::<4>().0 {
                    assert_eq!(*p, [200, 100, 50, alpha]);
                }
            }
        }
    }
    #[test]
    fn median_removes_impulses_and_morphology_matches_known_extrema() {
        let mut source = [10, 100, 200, 255].repeat(25);
        source[12 * 4..][..4].copy_from_slice(&[255; 4]);
        assert_eq!(
            run(&source, &Settings::new(Kind::Median, 5, 5)),
            [10, 100, 200, 255].repeat(25)
        );
        let source: Vec<u8> = [10, 20, 200, 30, 40]
            .into_iter()
            .flat_map(|v| [v, v, v, 255])
            .collect();
        let min = run(&source, &Settings::new(Kind::Minimum, 5, 1));
        let max = run(&source, &Settings::new(Kind::Maximum, 5, 1));
        assert_eq!(
            min.as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            [10, 10, 20, 30, 30]
        );
        assert_eq!(
            max.as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            [20, 200, 200, 200, 40]
        );
        let source = [255, 255, 255, 255, 0, 0, 0, 0, 255, 255, 255, 255];
        assert_eq!(run(&source, &Settings::new(Kind::Minimum, 3, 1)), source);
    }
    #[test]
    fn seeded_filters_are_repeatable_and_clouds_fill_empty_layers() {
        let source = vec![0; 16 * 12 * 4];
        let settings = Settings::new(Kind::Clouds, 16, 12);
        let a = run(&source, &settings);
        assert_eq!(a, run(&source, &settings));
        assert!(a.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        let mut changed = settings.clone();
        changed.seed = 777;
        assert_ne!(a, run(&source, &changed));
        changed.alpha_lock = true;
        assert_eq!(run(&source, &changed), source);
        let source = [100, 100, 100, 255].repeat(16 * 12);
        let settings = Settings::new(Kind::AddNoise, 16, 12);
        let a = run(&source, &settings);
        assert_eq!(a, run(&source, &settings));
        assert_ne!(a, source);
        for p in a.as_chunks::<4>().0 {
            assert_eq!(p[0], p[1]);
            assert_eq!(p[1], p[2]);
        }
    }
    #[test]
    fn offset_wrap_is_exact_and_soft_selection_uses_premultiplied_alpha() {
        let source = [255, 0, 0, 255, 0, 0, 0, 0, 0, 255, 0, 128];
        let mut settings = Settings::new(Kind::Offset, 3, 1);
        settings.p[0] = 1.0;
        settings.p[1] = 0.0;
        assert_eq!(
            run(&source, &settings),
            [0, 255, 0, 128, 255, 0, 0, 255, 0, 0, 0, 0]
        );
        let mask = Mask::from_values(3, 1, vec![0, 128, 255]).unwrap();
        let out = apply(&source, Some(&mask), &settings, &AtomicBool::new(false)).unwrap();
        assert_eq!(&out[..4], &source[..4]);
        assert_eq!(&out[4..8], &[255, 0, 0, 128]);
        assert_eq!(&out[8..], &[0, 0, 0, 0]);
    }
    #[test]
    fn zero_strength_spatial_filters_and_identity_kernel_are_identity() {
        let source = pattern(13, 9);
        for kind in [
            Kind::Pinch,
            Kind::Spherize,
            Kind::Twirl,
            Kind::Ripple,
            Kind::Wave,
            Kind::Glass,
            Kind::Diffuse,
            Kind::MotionBlur,
            Kind::RadialBlur,
            Kind::ZoomBlur,
            Kind::GaussianBlur,
            Kind::ShapeBlur,
        ] {
            let mut settings = Settings::new(kind, 13, 9);
            settings.p[0] = 0.0;
            let out = run(&source, &settings);
            for (src, dst) in source.as_chunks::<4>().0.iter().zip(out.as_chunks::<4>().0) {
                if src[3] > 0 {
                    for c in 0..4 {
                        assert!(src[c].abs_diff(dst[c]) <= 1, "{kind:?}: {src:?} {dst:?}");
                    }
                }
            }
        }
        assert_eq!(run(&source, &Settings::new(Kind::Custom, 13, 9)), source);
    }
    #[test]
    fn displacement_maps_are_used_and_invalid_inputs_are_rejected() {
        let source = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255];
        let mut settings = Settings::new(Kind::Displace, 3, 1);
        settings.p[0] = 1.0;
        settings.p[1] = 0.0;
        settings.edge = Edge::Wrap;
        settings.map = Some(Arc::new(Map {
            width: 1,
            height: 1,
            pixels: vec![255, 128, 128, 255],
        }));
        assert_eq!(
            run(&source, &settings),
            [0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255]
        );
        settings.width = 4;
        assert!(apply(&source, None, &settings, &AtomicBool::new(false)).is_err());
        settings.width = 3;
        settings.p[0] = f32::NAN;
        assert!(apply(&source, None, &settings, &AtomicBool::new(false)).is_err());
        let mut settings = Settings::new(Kind::Custom, 3, 1);
        settings.p[9] = 0.0;
        assert!(apply(&source, None, &settings, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn full_catalog_outputs_can_be_inspected_as_a_contact_sheet() {
        let source: Vec<u8> = (0..64 * 48)
            .flat_map(|i| {
                let x = i % 64;
                let y = i / 64;
                if (x as f32 - 32.0).powi(2) + (y as f32 - 24.0).powi(2) < 12.0f32.powi(2) {
                    [240, 160, 30, 255]
                } else {
                    [(x * 4) as u8, (y * 5) as u8, ((x + y) * 2) as u8, 255]
                }
            })
            .collect();
        let mut sheet = image::RgbaImage::new(64 * 8, 48 * 10);
        for (i, &kind) in Kind::ALL.iter().enumerate() {
            let output = run(&source, &Settings::new(kind, 64, 48));
            let image = image::RgbaImage::from_raw(64, 48, output).unwrap();
            image::imageops::replace(&mut sheet, &image, (i % 8 * 64) as i64, (i / 8 * 48) as i64);
        }
        if let Ok(path) = std::env::var("PAINTATLAS_FILTER_CONTACT_SHEET") {
            sheet.save(path).unwrap();
        }
    }
}

#[cfg(test)]
mod parameter_tests {
    use super::*;
    #[test]
    fn cropped_blur_matches_full_canvas_for_sparse_content_and_clamped_edges() {
        for origin in [(0, 0), (40, 30), (92, 72)] {
            let (w, h) = (101, 83);
            let mut source = vec![0; w * h * 4];
            for y in origin.1..origin.1 + 5 {
                for x in origin.0..origin.0 + 7 {
                    source[(y * w + x) * 4..][..4].copy_from_slice(&[
                        (x * 2) as u8,
                        (y * 3) as u8,
                        100,
                        170,
                    ]);
                }
            }
            for r in [1, 4] {
                for gaussian in [false, true] {
                    let result =
                        blurred(&source, w, h, r as f32, gaussian, &AtomicBool::new(false))
                            .unwrap();
                    let full =
                        blurred_full(&source, w, h, r, gaussian, &AtomicBool::new(false)).unwrap();
                    assert_eq!(
                        result, full,
                        "origin {origin:?} radius {r} gaussian {gaussian}"
                    );
                }
            }
        }
    }
    #[test]
    fn parallel_box_pass_matches_single_worker() {
        let (w, h) = (513, 513);
        let source: Vec<_> = (0..w * h)
            .map(|i| [(i % 65536) as u16, 65535, 70, ((i * 31) % 65536) as u16])
            .collect();
        for vertical in [false, true] {
            let mut reference = vec![[0; 4]; source.len()];
            let mut parallel = reference.clone();
            box_pass_rows(
                &source,
                &mut reference,
                [w, h, 0],
                24,
                vertical,
                &AtomicBool::new(false),
            )
            .unwrap();
            box_pass(
                &source,
                &mut parallel,
                w,
                h,
                24,
                vertical,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(parallel, reference);
        }
    }
    #[test]
    fn optimized_box_pass_matches_reference_at_edges_and_all_channel_ranges() {
        let cancel = AtomicBool::new(false);
        for (w, h) in [(1, 1), (1, 7), (9, 1), (17, 13)] {
            let src: Vec<[u16; 4]> = (0..w * h)
                .map(|i| {
                    [
                        ((i * 29137) % 65536) as u16,
                        65535,
                        (i % 2 * 65535) as u16,
                        (i * 37) as u16,
                    ]
                })
                .collect();
            for r in [0, 1, 4, 24, 128] {
                for vertical in [false, true] {
                    let mut output = vec![[0; 4]; src.len()];
                    box_pass(&src, &mut output, w, h, r, vertical, &cancel).unwrap();
                    for y in 0..h {
                        for x in 0..w {
                            let mut sum = [0_u64; 4];
                            for d in -(r as i32)..=r as i32 {
                                let sx = if vertical {
                                    x
                                } else {
                                    (x as i32 + d).clamp(0, w as i32 - 1) as usize
                                };
                                let sy = if vertical {
                                    (y as i32 + d).clamp(0, h as i32 - 1) as usize
                                } else {
                                    y
                                };
                                for c in 0..4 {
                                    sum[c] += src[sy * w + sx][c] as u64;
                                }
                            }
                            let count = (r * 2 + 1) as u64;
                            assert_eq!(
                                output[y * w + x],
                                sum.map(|v| ((v + count / 2) / count) as u16),
                                "{w}x{h} radius {r} vertical {vertical}"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn visible_gap_and_monochrome_controls_change_the_effect() {
        let source: Vec<u8> = (0..24 * 16)
            .flat_map(|i| {
                let x = i % 24;
                let y = i / 24;
                [(x * 10) as u8, (y * 16) as u8, 80, 255]
            })
            .collect();
        for kind in [
            Kind::Pointillize,
            Kind::Patchwork,
            Kind::MosaicTiles,
            Kind::StainedGlass,
        ] {
            let mut settings = Settings::new(kind, 24, 16);
            settings.p[1] = 0.0;
            let a = apply(&source, None, &settings, &AtomicBool::new(false)).unwrap();
            settings.p[1] = 45.0;
            let b = apply(&source, None, &settings, &AtomicBool::new(false)).unwrap();
            assert_ne!(a, b, "{kind:?}");
        }
        let mut settings = Settings::new(Kind::Mezzotint, 24, 16);
        let mono = apply(&source, None, &settings, &AtomicBool::new(false)).unwrap();
        assert!(
            mono.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2])
        );
        settings.p[1] = 1.0;
        let color = apply(&source, None, &settings, &AtomicBool::new(false)).unwrap();
        assert_ne!(mono, color);
    }
    #[test]
    #[ignore = "manual 3000px Gaussian processing benchmark"]
    fn gaussian_processing_benchmark() {
        let source: Vec<u8> = (0..9_000_000)
            .flat_map(|i| [(i % 251) as u8, (i % 239) as u8, 80, (i % 255) as u8])
            .collect();
        let mut settings = Settings::new(Kind::GaussianBlur, 3000, 3000);
        settings.p[0] = 24.0;
        let start = std::time::Instant::now();
        let result = apply(&source, None, &settings, &AtomicBool::new(false)).unwrap();
        eprintln!("Gaussian 3000x3000 radius 24: {:?}", start.elapsed());
        assert_eq!(result.len(), source.len());
        std::hint::black_box(result);
    }
}
