//! Local raster retouching. Dabs read an immutable source patch, never their own writes.
//! Bilinear interpolation uses premultiplied alpha to avoid dark transparent fringes.
use crate::document::{Brush, Document, blend};

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Mode {
    #[default]
    Push,
    TwirlLeft,
    TwirlRight,
    Pinch,
    Expand,
    Crystals,
    Edge,
    Reconstruct,
}
impl Mode {
    pub const ALL: [Self; 8] = [
        Self::Push,
        Self::TwirlLeft,
        Self::TwirlRight,
        Self::Pinch,
        Self::Expand,
        Self::Crystals,
        Self::Edge,
        Self::Reconstruct,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Push => "Push",
            Self::TwirlLeft => "Twirl left",
            Self::TwirlRight => "Twirl right",
            Self::Pinch => "Pinch",
            Self::Expand => "Expand",
            Self::Crystals => "Crystals",
            Self::Edge => "Edge",
            Self::Reconstruct => "Reconstruct",
        }
    }
}

pub fn copy_pixels(pixels: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.try_reserve_exact(pixels.len())
        .map_err(|e| format!("Could not allocate retouch source: {e}"))?;
    out.extend_from_slice(pixels);
    Ok(out)
}

struct Source<'a> {
    pixels: &'a [u8],
    width: usize,
    height: usize,
    origin: [usize; 2],
}
impl Source<'_> {
    fn sample(&self, p: [f32; 2]) -> [u8; 4] {
        let x = p[0] - self.origin[0] as f32 - 0.5;
        let y = p[1] - self.origin[1] as f32 - 0.5;
        let ix = x.floor() as isize;
        let iy = y.floor() as isize;
        let fx = x - x.floor();
        let fy = y - y.floor();
        let mut rgba = [0.0_f32; 4];
        for (dx, dy, weight) in [
            (0, 0, (1.0 - fx) * (1.0 - fy)),
            (1, 0, fx * (1.0 - fy)),
            (0, 1, (1.0 - fx) * fy),
            (1, 1, fx * fy),
        ] {
            if weight <= 0.0 {
                continue;
            }
            let (sx, sy) = (ix + dx, iy + dy);
            if sx < 0 || sy < 0 || sx >= self.width as isize || sy >= self.height as isize {
                continue;
            }
            let i = (sy as usize * self.width + sx as usize) * 4;
            let a = self.pixels[i + 3] as f32 / 255.0;
            for (c, value) in rgba[..3].iter_mut().enumerate() {
                *value += self.pixels[i + c] as f32 * a * weight;
            }
            rgba[3] += a * weight;
        }
        if rgba[3] <= 0.00001 {
            return [0; 4];
        }
        [
            (rgba[0] / rgba[3]).round() as u8,
            (rgba[1] / rgba[3]).round() as u8,
            (rgba[2] / rgba[3]).round() as u8,
            (rgba[3] * 255.0).round() as u8,
        ]
    }
}
fn bounds(doc: &Document, p: [f32; 2], radius: f32) -> Option<[usize; 4]> {
    let r = [
        (p[0] - radius - 1.0).floor().clamp(0.0, doc.width as f32) as usize,
        (p[1] - radius - 1.0).floor().clamp(0.0, doc.height as f32) as usize,
        (p[0] + radius + 1.0).ceil().clamp(0.0, doc.width as f32) as usize,
        (p[1] + radius + 1.0).ceil().clamp(0.0, doc.height as f32) as usize,
    ];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}
#[inline]
pub(crate) fn mix(dst: [u8; 4], src: [u8; 4], t: f32, locked: bool) -> [u8; 4] {
    if t >= 1.0 {
        if !locked {
            return if src[3] == 0 { [0; 4] } else { src };
        }
        return if dst[3] == 0 || src[3] == 0 {
            dst
        } else {
            [src[0], src[1], src[2], dst[3]]
        };
    }
    if locked && dst[3] == 0 {
        return dst;
    }
    let a = dst[3] as f32 * (1.0 - t) + src[3] as f32 * t;
    if a < 0.001 {
        return if locked { dst } else { [0; 4] };
    }
    let mut out = [0; 4];
    for c in 0..3 {
        out[c] = ((dst[c] as f32 * dst[3] as f32 * (1.0 - t) + src[c] as f32 * src[3] as f32 * t)
            / a)
            .round() as u8;
    }
    out[3] = if locked { dst[3] } else { a.round() as u8 };
    out
}

// Keep geometry in floating point and resample colors separately. Quantizing each
// dab's displacement loses slow movements; repeatedly resampling colors blurs them.
const WARP_TILE: usize = 64;
struct WarpTile {
    original: Vec<u8>,
    displacement: Vec<[f32; 2]>,
}
#[derive(Default)]
struct Warp {
    // Dense indices, sparse tile storage: no tree lookup in the per-pixel path.
    tiles: Vec<Option<Box<WarpTile>>>,
    columns: usize,
    touched: Vec<usize>,
}
impl Warp {
    fn clear(&mut self) {
        for index in self.touched.drain(..) {
            self.tiles[index] = None;
        }
    }
    fn capture(&mut self, doc: &Document, r: [usize; 4]) -> Result<(), String> {
        let columns = (doc.width as usize).div_ceil(WARP_TILE);
        let rows = (doc.height as usize).div_ceil(WARP_TILE);
        if self.columns != columns || self.tiles.len() != columns * rows {
            self.clear();
            self.tiles.clear();
            self.tiles
                .try_reserve_exact(columns * rows)
                .map_err(|e| format!("Could not allocate Liquify tile index: {e}"))?;
            self.tiles.resize_with(columns * rows, || None);
            self.columns = columns;
        }
        for ty in r[1] / WARP_TILE..=(r[3] - 1) / WARP_TILE {
            for tx in r[0] / WARP_TILE..=(r[2] - 1) / WARP_TILE {
                let index = ty * columns + tx;
                if self.tiles[index].is_some() {
                    continue;
                }
                self.touched
                    .try_reserve(1)
                    .map_err(|e| format!("Could not allocate Liquify touched-tile list: {e}"))?;
                let mut tile = WarpTile {
                    original: Vec::new(),
                    displacement: Vec::new(),
                };
                tile.original
                    .try_reserve_exact(WARP_TILE * WARP_TILE * 4)
                    .map_err(|e| format!("Could not allocate Liquify source tile: {e}"))?;
                tile.displacement
                    .try_reserve_exact(WARP_TILE * WARP_TILE)
                    .map_err(|e| format!("Could not allocate Liquify displacement tile: {e}"))?;
                tile.original.resize(WARP_TILE * WARP_TILE * 4, 0);
                tile.displacement.resize(WARP_TILE * WARP_TILE, [0.0; 2]);
                let x = tx * WARP_TILE;
                let y = ty * WARP_TILE;
                let w = WARP_TILE.min(doc.width as usize - x);
                let h = WARP_TILE.min(doc.height as usize - y);
                for row in 0..h {
                    let i = ((y + row) * doc.width as usize + x) * 4;
                    let j = row * WARP_TILE * 4;
                    tile.original[j..j + w * 4]
                        .copy_from_slice(&doc.layers[doc.active].pixels[i..i + w * 4]);
                }
                self.tiles[index] = Some(Box::new(tile));
                self.touched.push(index);
            }
        }
        Ok(())
    }
    fn copy_field(&self, r: [usize; 4], out: &mut [[f32; 2]]) {
        let width = r[2] - r[0];
        for y in r[1]..r[3] {
            let mut x = r[0];
            while x < r[2] {
                let count = (WARP_TILE - x % WARP_TILE).min(r[2] - x);
                let dst = (y - r[1]) * width + x - r[0];
                if let Some(tile) = &self.tiles[(y / WARP_TILE) * self.columns + x / WARP_TILE] {
                    let src = (y % WARP_TILE) * WARP_TILE + x % WARP_TILE;
                    out[dst..dst + count].copy_from_slice(&tile.displacement[src..src + count]);
                } else {
                    out[dst..dst + count].fill([0.0; 2]);
                }
                x += count;
            }
        }
    }
    #[inline]
    fn nearest(&self, p: [f32; 2], live: &[u8], width: usize, height: usize) -> [u8; 4] {
        if p[0] < 0.0 || p[1] < 0.0 || p[0] >= width as f32 || p[1] >= height as f32 {
            return [0; 4];
        }
        let (x, y) = (p[0] as usize, p[1] as usize);
        if let Some(tile) = &self.tiles[(y / WARP_TILE) * self.columns + x / WARP_TILE] {
            let i = ((y % WARP_TILE) * WARP_TILE + x % WARP_TILE) * 4;
            tile.original[i..i + 4].try_into().unwrap()
        } else {
            let i = (y * width + x) * 4;
            live[i..i + 4].try_into().unwrap()
        }
    }
    fn write(
        &mut self,
        r: [usize; 4],
        out: &[WarpOutput],
        pixels: &mut [u8],
        width: usize,
        changed: bool,
    ) {
        let row_width = r[2] - r[0];
        for y in r[1]..r[3] {
            let row = &out[(y - r[1]) * row_width..(y - r[1] + 1) * row_width];
            let mut x = r[0];
            while x < r[2] {
                let count = (WARP_TILE - x % WARP_TILE).min(r[2] - x);
                let tile = self.tiles[(y / WARP_TILE) * self.columns + x / WARP_TILE]
                    .as_mut()
                    .unwrap();
                let dst = (y % WARP_TILE) * WARP_TILE + x % WARP_TILE;
                for (field, result) in tile.displacement[dst..dst + count]
                    .iter_mut()
                    .zip(&row[x - r[0]..x - r[0] + count])
                {
                    *field = result.displacement;
                }
                x += count;
            }
            if changed {
                let start = (y * width + r[0]) * 4;
                for (pixel, result) in pixels[start..start + row_width * 4]
                    .chunks_exact_mut(4)
                    .zip(row)
                {
                    pixel.copy_from_slice(&result.pixel);
                }
            }
        }
    }
    fn memory_bytes(&self) -> usize {
        self.tiles.capacity() * std::mem::size_of::<Option<Box<WarpTile>>>()
            + self.touched.capacity() * std::mem::size_of::<usize>()
            + self
                .tiles
                .iter()
                .flatten()
                .map(|t| t.original.capacity() + t.displacement.capacity() * 8)
                .sum::<usize>()
    }
}
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct WarpOutput {
    displacement: [f32; 2],
    pixel: [u8; 4],
}
#[derive(Clone, Copy)]
struct FieldPatch<'a> {
    values: &'a [[f32; 2]],
    rect: [usize; 4],
}
impl FieldPatch<'_> {
    #[inline]
    fn sample(&self, p: [f32; 2]) -> [f32; 2] {
        let x = p[0] - 0.5;
        let y = p[1] - 0.5;
        // Exact floor over canvas coordinates without repeated floorf calls in
        // the CPU hot loop (negative halo coordinates still round down).
        let mut ix = x as isize;
        let mut iy = y as isize;
        if ix as f32 > x {
            ix -= 1;
        }
        if iy as f32 > y {
            iy -= 1;
        }
        let fx = x - ix as f32;
        let fy = y - iy as f32;
        if ix >= self.rect[0] as isize
            && iy >= self.rect[1] as isize
            && ix + 1 < self.rect[2] as isize
            && iy + 1 < self.rect[3] as isize
        {
            let width = self.rect[2] - self.rect[0];
            let i = (iy as usize - self.rect[1]) * width + ix as usize - self.rect[0];
            let [a, b, c, d] = [
                self.values[i],
                self.values[i + 1],
                self.values[i + width],
                self.values[i + width + 1],
            ];
            return [
                (a[0] * (1.0 - fx) + b[0] * fx) * (1.0 - fy) + (c[0] * (1.0 - fx) + d[0] * fx) * fy,
                (a[1] * (1.0 - fx) + b[1] * fx) * (1.0 - fy) + (c[1] * (1.0 - fx) + d[1] * fx) * fy,
            ];
        }
        let mut result = [0.0; 2];
        for (dx, dy, w) in [
            (0, 0, (1.0 - fx) * (1.0 - fy)),
            (1, 0, fx * (1.0 - fy)),
            (0, 1, (1.0 - fx) * fy),
            (1, 1, fx * fy),
        ] {
            let (sx, sy) = (ix + dx, iy + dy);
            if w == 0.0
                || sx < self.rect[0] as isize
                || sy < self.rect[1] as isize
                || sx >= self.rect[2] as isize
                || sy >= self.rect[3] as isize
            {
                continue;
            }
            let v = self.values[(sy as usize - self.rect[1]) * (self.rect[2] - self.rect[0])
                + sx as usize
                - self.rect[0]];
            result[0] += v[0] * w;
            result[1] += v[1] * w;
        }
        result
    }
}
fn retouch_workers() -> usize {
    static COUNT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *COUNT.get_or_init(|| std::thread::available_parallelism().map_or(1, |n| n.get().min(8)))
}
struct CrispKernel<'a> {
    rect: [usize; 4],
    field: FieldPatch<'a>,
    source: &'a Warp,
    live: &'a [u8],
    width: usize,
    height: usize,
    selection: Option<&'a crate::selection::Mask>,
    locked: bool,
    p: [f32; 2],
    delta: [f32; 2],
    radius: f32,
    strength: f32,
    distortion: f32,
    mode: Mode,
}
impl CrispKernel<'_> {
    fn rows(&self, output: &mut [WarpOutput], start_row: usize) -> bool {
        let mut changed = false;
        let row_width = self.rect[2] - self.rect[0];
        let radius2 = self.radius * self.radius;
        let length = self.delta[0].hypot(self.delta[1]);
        let direction = if length > 0.0001 {
            [self.delta[0] / length, self.delta[1] / length]
        } else {
            [1.0, 0.0]
        };
        for (row, results) in output.chunks_mut(row_width).enumerate() {
            let y = self.rect[1] + start_row + row;
            let dy = y as f32 + 0.5 - self.p[1];
            for (column, result) in results.iter_mut().enumerate() {
                let x = self.rect[0] + column;
                let i = (y * self.width + x) * 4;
                let old: [u8; 4] = self.live[i..i + 4].try_into().unwrap();
                let field_index =
                    (y - self.field.rect[1]) * (self.field.rect[2] - self.field.rect[0]) + x
                        - self.field.rect[0];
                *result = WarpOutput {
                    pixel: old,
                    displacement: self.field.values[field_index],
                };
                let dx = x as f32 + 0.5 - self.p[0];
                let d2 = (dx * dx + dy * dy) / radius2;
                if d2 >= 1.0 {
                    continue;
                }
                let selection = self.selection.map_or(1.0, |m| m.coverage(x, y));
                if selection == 0.0 {
                    continue;
                }
                let falloff = (1.0 - d2).powi(2);
                let noise = if self.distortion > 0.0 {
                    1.0 + self.distortion
                        * ((dx * 0.073 + dy * 0.097).sin() * (dy * 0.061 - dx * 0.043).cos())
                } else {
                    1.0
                };
                let k = (self.strength * falloff * noise).clamp(0.0, 1.0);
                let center = [x as f32 + 0.5, y as f32 + 0.5];
                let mut q = center;
                match self.mode {
                    Mode::Push => {
                        q[0] -= self.delta[0] * k;
                        q[1] -= self.delta[1] * k;
                    }
                    Mode::TwirlLeft | Mode::TwirlRight => {
                        let angle = k
                            * 0.35
                            * if self.mode == Mode::TwirlLeft {
                                1.0
                            } else {
                                -1.0
                            };
                        let (s, c) = angle.sin_cos();
                        q = [self.p[0] + dx * c - dy * s, self.p[1] + dx * s + dy * c];
                    }
                    Mode::Pinch | Mode::Expand | Mode::Crystals => {
                        let shards = if self.mode == Mode::Crystals {
                            1.0 + 0.8 * (dy.atan2(dx) * 9.0).sin()
                        } else {
                            1.0
                        };
                        let sign = if self.mode == Mode::Pinch { 1.0 } else { -1.0 };
                        let scale = 1.0 + sign * k * 0.22 * shards;
                        q = [self.p[0] + dx * scale, self.p[1] + dy * scale];
                    }
                    Mode::Edge => {
                        let normal = [-direction[1], direction[0]];
                        let distance = dx * normal[0] + dy * normal[1];
                        q[0] += normal[0] * distance * k * 0.35;
                        q[1] += normal[1] * distance * k * 0.35;
                    }
                    Mode::Reconstruct => unreachable!(),
                }
                q = [
                    center[0] + (q[0] - center[0]) * selection,
                    center[1] + (q[1] - center[1]) * selection,
                ];
                let previous = self.field.sample(q);
                let mapped = [q[0] + previous[0], q[1] + previous[1]];
                result.displacement = [mapped[0] - center[0], mapped[1] - center[1]];
                result.pixel = mix(
                    old,
                    self.source
                        .nearest(mapped, self.live, self.width, self.height),
                    1.0,
                    self.locked,
                );
                changed |= result.pixel != old;
            }
        }
        changed
    }
}
#[derive(Default)]
pub struct Scratch {
    pixels: Vec<u8>,
    field_patch: Vec<[f32; 2]>,
    warp: Warp,
    output: Vec<WarpOutput>,
}
impl Scratch {
    pub fn begin_stroke(&mut self) {
        self.end_stroke();
    }
    pub fn end_stroke(&mut self) {
        self.warp.clear();
    }
    pub fn memory_bytes(&self) -> usize {
        self.pixels.capacity()
            + self.field_patch.capacity() * 8
            + self.warp.memory_bytes()
            + self.output.capacity() * std::mem::size_of::<WarpOutput>()
    }
    /// dt_strength is normalized to seconds by the caller, so a stationary pen
    /// deforms at the same speed regardless of display refresh rate.
    pub fn liquify(
        &mut self,
        doc: &mut Document,
        p: [f32; 2],
        delta: [f32; 2],
        size: f32,
        strength: f32,
        distortion: f32,
        mode: Mode,
        original: &[u8],
        smoothing: bool,
    ) -> Result<bool, String> {
        if strength <= 0.0 {
            return Ok(false);
        }
        let radius = (size * 0.5).max(0.5);
        let distance = delta[0].hypot(delta[1]);
        let delta = if distance > radius * 0.45 {
            [
                delta[0] * radius * 0.45 / distance,
                delta[1] * radius * 0.45 / distance,
            ]
        } else {
            delta
        };
        if mode == Mode::Push && delta[0].hypot(delta[1]) < 0.0001 {
            return Ok(false);
        }
        let Some(r) = bounds(doc, p, radius) else {
            return Ok(false);
        };
        // The largest inverse displacement is less than 0.45 radii. Include
        // interpolation neighbors so the patch is equivalent to a full-layer read.
        let max_displacement = if mode == Mode::Push {
            delta[0].hypot(delta[1]) * (strength * (1.0 + distortion)).clamp(0.0, 1.0)
        } else {
            radius * 0.5
        };
        let pad = max_displacement.ceil() as usize + 2;
        let patch = [
            r[0].saturating_sub(pad),
            r[1].saturating_sub(pad),
            (r[2] + pad).min(doc.width as usize),
            (r[3] + pad).min(doc.height as usize),
        ];
        let (w, h) = (patch[2] - patch[0], patch[3] - patch[1]);
        let continuous = !smoothing && mode != Mode::Reconstruct;
        if continuous {
            self.warp.capture(doc, r)?;
            self.field_patch
                .try_reserve((w * h).saturating_sub(self.field_patch.len()))
                .map_err(|e| format!("Could not allocate Liquify field patch: {e}"))?;
            self.field_patch.resize(w * h, [0.0; 2]);
            self.warp.copy_field(patch, &mut self.field_patch);
        } else {
            self.pixels.clear();
            self.pixels
                .try_reserve(w * h * 4)
                .map_err(|e| format!("Could not allocate Liquify patch: {e}"))?;
            for y in patch[1]..patch[3] {
                let i = (y * doc.width as usize + patch[0]) * 4;
                self.pixels
                    .extend_from_slice(&doc.layers[doc.active].pixels[i..i + w * 4]);
            }
        }
        let field = FieldPatch {
            values: &self.field_patch,
            rect: patch,
        };
        let source = Source {
            pixels: &self.pixels,
            width: w,
            height: h,
            origin: [patch[0], patch[1]],
        };
        let original = Source {
            pixels: original,
            width: doc.width as usize,
            height: doc.height as usize,
            origin: [0, 0],
        };
        doc.prepare_pixel_edit(r);
        let locked = doc.layers[doc.active].alpha_lock;
        if continuous {
            let count = (r[2] - r[0]) * (r[3] - r[1]);
            self.output
                .try_reserve(count.saturating_sub(self.output.len()))
                .map_err(|e| format!("Could not allocate Liquify output: {e}"))?;
            self.output.resize(count, WarpOutput::default());
            let source = &self.warp;
            let live = &doc.layers[doc.active].pixels;
            let selection = doc.selection.as_deref();
            let width = doc.width as usize;
            let height = doc.height as usize;
            let kernel = CrispKernel {
                rect: r,
                field,
                source,
                live,
                width,
                height,
                selection,
                locked,
                p,
                delta,
                radius,
                strength,
                distortion,
                mode,
            };
            let compute =
                |output: &mut [WarpOutput], start_row: usize| kernel.rows(output, start_row);
            let workers = if count >= 128 * 128 {
                retouch_workers().min((r[3] - r[1]).div_ceil(32))
            } else {
                1
            };
            let changed = if workers > 1 {
                let rows = (r[3] - r[1]).div_ceil(workers);
                std::thread::scope(|scope| {
                    let mut chunks = self.output.chunks_mut(rows * (r[2] - r[0]));
                    let first = chunks.next().unwrap();
                    let mut jobs = Vec::with_capacity(workers - 1);
                    for (i, chunk) in chunks.enumerate() {
                        let compute = &compute;
                        jobs.push(scope.spawn(move || compute(chunk, (i + 1) * rows)));
                    }
                    let changed = compute(first, 0);
                    jobs.into_iter().fold(changed, |changed, job| {
                        job.join().expect("Liquify worker panicked") || changed
                    })
                })
            } else {
                compute(&mut self.output, 0)
            };
            self.warp.write(
                r,
                &self.output,
                &mut doc.layers[doc.active].pixels,
                width,
                changed,
            );
            return Ok(changed);
        }
        let pixels = &mut doc.layers[doc.active].pixels;
        let mut changed = false;
        let length = delta[0].hypot(delta[1]);
        let direction = if length > 0.0001 {
            [delta[0] / length, delta[1] / length]
        } else {
            [1.0, 0.0]
        };
        for y in r[1]..r[3] {
            for x in r[0]..r[2] {
                let (dx, dy) = (x as f32 + 0.5 - p[0], y as f32 + 0.5 - p[1]);
                let d2 = (dx * dx + dy * dy) / (radius * radius);
                if d2 >= 1.0 {
                    continue;
                }
                let falloff = (1.0 - d2).powi(2);
                let selection = doc.selection.as_ref().map_or(1.0, |m| m.coverage(x, y));
                if selection == 0.0 {
                    continue;
                }
                let noise = if distortion > 0.0 {
                    1.0 + distortion
                        * ((dx * 0.073 + dy * 0.097).sin() * (dy * 0.061 - dx * 0.043).cos())
                } else {
                    1.0
                };
                let k = (strength * falloff * noise).clamp(0.0, 1.0);
                let mut q = [x as f32 + 0.5, y as f32 + 0.5];
                match mode {
                    Mode::Push => {
                        q[0] -= delta[0] * k;
                        q[1] -= delta[1] * k;
                    }
                    Mode::TwirlLeft | Mode::TwirlRight => {
                        let angle = k * 0.35 * if mode == Mode::TwirlLeft { 1.0 } else { -1.0 };
                        let (s, c) = angle.sin_cos();
                        q = [p[0] + dx * c - dy * s, p[1] + dx * s + dy * c];
                    }
                    Mode::Pinch | Mode::Expand | Mode::Crystals => {
                        let shards = if mode == Mode::Crystals {
                            1.0 + 0.8 * (dy.atan2(dx) * 9.0).sin()
                        } else {
                            1.0
                        };
                        let sign = if mode == Mode::Pinch { 1.0 } else { -1.0 };
                        let scale = 1.0 + sign * k * 0.22 * shards;
                        q = [p[0] + dx * scale, p[1] + dy * scale];
                    }
                    Mode::Edge => {
                        let normal = [-direction[1], direction[0]];
                        let distance = dx * normal[0] + dy * normal[1];
                        q[0] += normal[0] * distance * k * 0.35;
                        q[1] += normal[1] * distance * k * 0.35;
                    }
                    Mode::Reconstruct => {}
                }
                let i = (y * doc.width as usize + x) * 4;
                let old: [u8; 4] = pixels[i..i + 4].try_into().unwrap();
                let out = if mode == Mode::Reconstruct {
                    mix(old, original.sample(q), k * selection, locked)
                } else {
                    mix(old, source.sample(q), selection, locked)
                };
                if out != old {
                    pixels[i..i + 4].copy_from_slice(&out);
                    changed = true;
                }
            }
        }
        Ok(changed)
    }
}

pub fn clone_dab(
    doc: &mut Document,
    original: &[u8],
    p: [f32; 2],
    source_point: [f32; 2],
    brush: &Brush,
) -> bool {
    if brush.opacity <= 0.0 {
        return false;
    }
    let radius = (brush.size * 0.5).max(0.5);
    let Some(r) = bounds(doc, p, radius) else {
        return false;
    };
    let source = Source {
        pixels: original,
        width: doc.width as usize,
        height: doc.height as usize,
        origin: [0, 0],
    };
    doc.prepare_pixel_edit(r);
    let mask = brush.tip.as_ref().map(|tip| tip.mask(brush.size));
    let proportions = brush
        .tip
        .as_ref()
        .map_or([1.0, 1.0], |tip| tip.proportions());
    let locked = doc.layers[doc.active].alpha_lock;
    let pixels = &mut doc.layers[doc.active].pixels;
    let mut changed = false;
    for y in r[1]..r[3] {
        for x in r[0]..r[2] {
            let (dx, dy) = (x as f32 + 0.5 - p[0], y as f32 + 0.5 - p[1]);
            let coverage = if let Some(mask) = mask {
                let a = mask.sample(
                    dx / (brush.size * proportions[0]) + 0.5,
                    dy / (brush.size * proportions[1]) + 0.5,
                );
                a * (brush.hardness + (1.0 - brush.hardness) * a)
            } else {
                let d = dx.hypot(dy);
                let edge = (radius - d + 0.5).clamp(0.0, 1.0);
                let softness = if brush.hardness >= 0.99 {
                    1.0
                } else {
                    ((radius - d) / (radius * (1.0 - brush.hardness)).max(0.5)).clamp(0.0, 1.0)
                };
                edge * softness
            };
            let alpha =
                coverage * brush.opacity * doc.selection.as_ref().map_or(1.0, |m| m.coverage(x, y));
            if alpha <= 0.0 {
                continue;
            }
            let color = source.sample([source_point[0] + dx, source_point[1] + dy]);
            let i = (y * doc.width as usize + x) * 4;
            let old: [u8; 4] = pixels[i..i + 4].try_into().unwrap();
            let mut out = old;
            if locked {
                if old[3] == 0 {
                    continue;
                }
                let t = alpha * color[3] as f32 / 255.0;
                for c in 0..3 {
                    out[c] = (old[c] as f32 * (1.0 - t) + color[c] as f32 * t).round() as u8;
                }
            } else {
                blend(&mut out, color, alpha);
            }
            if out != old {
                pixels[i..i + 4].copy_from_slice(&out);
                changed = true;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{history::History, selection::Mask};
    use eframe::egui::{Pos2, Rect};
    fn gradient() -> Document {
        let mut doc = Document::new(96, 96, true);
        let layer = &mut doc.layers[doc.active];
        for y in 0..96 {
            for x in 0..96 {
                let i = (y * 96 + x) * 4;
                layer.pixels[i..i + 4].copy_from_slice(&[x as u8 * 2, y as u8 * 2, 120, 255]);
            }
        }
        doc
    }
    fn at(doc: &Document, x: usize, y: usize) -> [u8; 4] {
        doc.layers[doc.active].pixels
            [(y * doc.width as usize + x) * 4..(y * doc.width as usize + x) * 4 + 4]
            .try_into()
            .unwrap()
    }
    fn brush(size: f32) -> Brush {
        Brush {
            color: [0, 0, 0, 255],
            size,
            opacity: 1.0,
            hardness: 1.0,
            tip: None,
            spacing: 0.2,
        }
    }
    #[test]
    fn crisp_liquify_accumulates_subpixel_motion_in_both_directions() {
        for delta in [[0.2, 0.0], [-0.2, 0.0], [0.0, 0.2], [0.0, -0.2]] {
            let mut doc = gradient();
            let original = doc.layers[doc.active].pixels.clone();
            let mut scratch = Scratch::default();
            let mut history = History::new();
            history.push_stroke(&mut doc);
            // Each dab alone is far below the half-pixel rounding threshold.
            scratch
                .liquify(
                    &mut doc,
                    [48.5, 48.5],
                    delta,
                    48.0,
                    0.2,
                    0.0,
                    Mode::Push,
                    &original,
                    false,
                )
                .unwrap();
            assert_eq!(at(&doc, 48, 48), [96, 96, 120, 255]);
            for _ in 0..39 {
                scratch
                    .liquify(
                        &mut doc,
                        [48.5, 48.5],
                        delta,
                        48.0,
                        0.2,
                        0.0,
                        Mode::Push,
                        &original,
                        false,
                    )
                    .unwrap();
            }
            assert_ne!(
                at(&doc, 48, 48),
                [96, 96, 120, 255],
                "slow drag {delta:?} must move pixels"
            );
            // Sampling the immutable source must not invent intermediate colors.
            assert!(
                doc.layers[doc.active]
                    .pixels
                    .chunks_exact(4)
                    .all(|p| p[0] % 2 == 0 && p[1] % 2 == 0 && p[2] == 120 && p[3] == 255)
            );
            let after = doc.layers[doc.active].pixels.clone();
            history.commit(&doc);
            assert!(history.undo(&mut doc));
            assert_eq!(doc.layers[doc.active].pixels, original);
            assert!(history.redo(&mut doc));
            assert_eq!(doc.layers[doc.active].pixels, after);
        }
    }
    #[test]
    fn crisp_stationary_modes_accumulate_weak_effects() {
        for mode in Mode::ALL
            .into_iter()
            .filter(|m| !matches!(m, Mode::Push | Mode::Reconstruct))
        {
            let mut doc = gradient();
            let original = doc.layers[doc.active].pixels.clone();
            let mut scratch = Scratch::default();
            for _ in 0..100 {
                scratch
                    .liquify(
                        &mut doc,
                        [48.5, 48.5],
                        [0.0; 2],
                        48.0,
                        0.02,
                        0.0,
                        mode,
                        &original,
                        false,
                    )
                    .unwrap();
            }
            assert_ne!(
                doc.layers[doc.active].pixels, original,
                "{mode:?} must respond without smoothing"
            );
        }
    }
    #[test]
    fn starting_another_crisp_stroke_uses_its_current_pixels_as_source() {
        let mut doc = gradient();
        let original = doc.layers[doc.active].pixels.clone();
        let mut scratch = Scratch::default();
        scratch
            .liquify(
                &mut doc,
                [48.5, 48.5],
                [4.0, 0.0],
                48.0,
                1.0,
                0.0,
                Mode::Push,
                &original,
                false,
            )
            .unwrap();
        let mut expected = doc.clone();
        let mut fresh = Scratch::default();
        scratch.begin_stroke();
        for _ in 0..40 {
            scratch
                .liquify(
                    &mut doc,
                    [48.5, 48.5],
                    [0.0, 0.2],
                    48.0,
                    0.2,
                    0.0,
                    Mode::Push,
                    &original,
                    false,
                )
                .unwrap();
            fresh
                .liquify(
                    &mut expected,
                    [48.5, 48.5],
                    [0.0, 0.2],
                    48.0,
                    0.2,
                    0.0,
                    Mode::Push,
                    &original,
                    false,
                )
                .unwrap();
        }
        assert_eq!(
            doc.layers[doc.active].pixels,
            expected.layers[expected.active].pixels
        );
    }
    #[test]
    fn parallel_kernel_matches_serial_for_modes_masks_alpha_and_partial_rows() {
        let mut doc = Document::new(256, 256, true);
        for (i, p) in doc.layers[doc.active]
            .pixels
            .chunks_exact_mut(4)
            .enumerate()
        {
            p.copy_from_slice(&[
                (i % 256) as u8,
                (i / 256) as u8,
                120,
                if i % 7 == 0 { 0 } else { 180 },
            ]);
        }
        let rect = [128, 32, 248, 224];
        let mut source = Warp::default();
        source.capture(&doc, rect).unwrap();
        let field_values: Vec<_> = (0..256 * 256)
            .map(|i| [(i % 13) as f32 * 0.03, (i % 7) as f32 * 0.04])
            .collect();
        let field = FieldPatch {
            values: &field_values,
            rect: [0, 0, 256, 256],
        };
        let selection = Mask::try_rectangle(
            256,
            256,
            Rect::from_min_max(Pos2::new(140.0, 50.0), Pos2::new(240.0, 210.0)),
        )
        .unwrap();
        for mode in Mode::ALL.into_iter().filter(|m| *m != Mode::Reconstruct) {
            for locked in [false, true] {
                let kernel = CrispKernel {
                    rect,
                    field,
                    source: &source,
                    live: &doc.layers[doc.active].pixels,
                    width: 256,
                    height: 256,
                    selection: selection.as_ref(),
                    locked,
                    p: [170.0, 128.0],
                    delta: [35.0, 5.0],
                    radius: 100.0,
                    strength: 0.7,
                    distortion: 0.3,
                    mode,
                };
                let mut serial =
                    vec![WarpOutput::default(); (rect[2] - rect[0]) * (rect[3] - rect[1])];
                let serial_changed = kernel.rows(&mut serial, 0);
                let mut parallel = vec![WarpOutput::default(); serial.len()];
                let parallel_changed = std::thread::scope(|scope| {
                    let kernel = &kernel;
                    let jobs: Vec<_> = parallel
                        .chunks_mut((rect[2] - rect[0]) * 53)
                        .enumerate()
                        .map(|(i, chunk)| scope.spawn(move || kernel.rows(chunk, i * 53)))
                        .collect();
                    jobs.into_iter().fold(false, |a, j| j.join().unwrap() || a)
                });
                assert_eq!(serial_changed, parallel_changed);
                assert!(serial_changed);
                assert_eq!(serial, parallel, "mode={mode:?}, alpha lock={locked}");
            }
        }
    }
    #[test]
    fn smoothing_can_be_disabled_without_blending_neighboring_pixels() {
        let mut crisp = Document::new(32, 32, true);
        for y in 0..32 {
            for x in 0..32 {
                let value = if (x + y) % 2 == 0 { 0 } else { 255 };
                let i = (y * 32 + x) * 4;
                crisp.layers[crisp.active].pixels[i..i + 4]
                    .copy_from_slice(&[value, value, value, 255]);
            }
        }
        let mut smooth = crisp.clone();
        let original = crisp.layers[crisp.active].pixels.clone();
        let mut scratch = Scratch::default();
        assert!(
            scratch
                .liquify(
                    &mut crisp,
                    [16.5, 16.5],
                    [1.25, 0.0],
                    24.0,
                    1.0,
                    0.0,
                    Mode::Push,
                    &original,
                    false
                )
                .unwrap()
        );
        assert!(
            scratch
                .liquify(
                    &mut smooth,
                    [16.5, 16.5],
                    [1.25, 0.0],
                    24.0,
                    1.0,
                    0.0,
                    Mode::Push,
                    &original,
                    true
                )
                .unwrap()
        );
        assert!(
            crisp.layers[crisp.active]
                .pixels
                .chunks_exact(4)
                .all(|p| p[0] == 0 || p[0] == 255)
        );
        assert!(
            smooth.layers[smooth.active]
                .pixels
                .chunks_exact(4)
                .any(|p| p[0] > 0 && p[0] < 255)
        );
        assert_ne!(crisp.layers[crisp.active].pixels, original);
    }
    #[test]
    #[ignore = "manual Liquify performance comparison"]
    fn benchmark_crisp_liquify_3000() {
        let mut base = Document::new(3000, 3000, true);
        base.dpi = 300;
        for (i, p) in base.layers[base.active]
            .pixels
            .chunks_exact_mut(4)
            .enumerate()
        {
            p.copy_from_slice(&[(i % 3000 % 256) as u8, (i / 3000 % 256) as u8, 120, 255]);
        }
        let original = base.layers[base.active].pixels.clone();
        for size in [100.0, 180.0, 500.0, 1000.0] {
            let mut doc = base.clone();
            let mut history = History::new();
            history.push_stroke(&mut doc);
            let mut scratch = Scratch::default();
            let start = std::time::Instant::now();
            for i in 0..12 {
                std::hint::black_box(
                    scratch
                        .liquify(
                            &mut doc,
                            [1500.0 + i as f32 * 3.0, 1500.0],
                            [3.0, 0.0],
                            size,
                            0.5,
                            0.0,
                            Mode::Push,
                            &original,
                            false,
                        )
                        .unwrap(),
                );
            }
            eprintln!(
                "LIQUIFY_BENCH size={size:.0} ms_per_dab={:.3} scratch_bytes={}",
                start.elapsed().as_secs_f64() * 1000.0 / 12.0,
                scratch.memory_bytes()
            );
        }
    }
    #[test]
    fn large_canvas_uses_a_local_patch_and_local_undo_tiles() {
        for smoothing in [true, false] {
            let mut doc = Document::new(3000, 3000, true);
            let original = vec![0; 3000 * 3000 * 4];
            let mut history = History::new();
            history.push_stroke(&mut doc);
            let mut scratch = Scratch::default();
            let start = std::time::Instant::now();
            for i in 0..16 {
                scratch
                    .liquify(
                        &mut doc,
                        [1500.0 + i as f32, 1500.0],
                        [1.0, 0.0],
                        100.0,
                        0.5,
                        0.0,
                        Mode::Push,
                        &original,
                        smoothing,
                    )
                    .unwrap();
            }
            eprintln!(
                "3000px canvas, smoothing={smoothing}, 100px Liquify brush: {:.2} ms/dab, {} scratch bytes",
                start.elapsed().as_secs_f64() * 1000.0 / 16.0,
                scratch.memory_bytes()
            );
            assert!(scratch.memory_bytes() < 1024 * 1024);
            assert!(doc.stroke_tiles.borrow().as_ref().unwrap().len() <= 4);
        }
    }
    #[test]
    fn push_moves_pixels_in_stroke_direction_and_only_captures_local_tiles() {
        let mut doc = gradient();
        let original = doc.layers[doc.active].pixels.clone();
        let mut history = History::new();
        history.push_stroke(&mut doc);
        let mut scratch = Scratch::default();
        assert!(
            scratch
                .liquify(
                    &mut doc,
                    [48.5, 48.5],
                    [8.0, 0.0],
                    40.0,
                    1.0,
                    0.0,
                    Mode::Push,
                    &original,
                    true,
                )
                .unwrap()
        );
        assert_eq!(at(&doc, 48, 48), [80, 96, 120, 255]);
        assert_eq!(at(&doc, 1, 1), [2, 2, 120, 255]);
        history.commit(&doc);
        let edited = doc.layers[doc.active].pixels.clone();
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[doc.active].pixels, original);
        assert!(history.redo(&mut doc));
        assert_eq!(doc.layers[doc.active].pixels, edited);
    }
    #[test]
    fn every_mode_has_effect_and_reconstruct_restores_original() {
        for mode in Mode::ALL.into_iter().filter(|m| *m != Mode::Reconstruct) {
            let mut doc = gradient();
            let original = doc.layers[doc.active].pixels.clone();
            let mut scratch = Scratch::default();
            assert!(
                scratch
                    .liquify(
                        &mut doc,
                        [48.5, 48.5],
                        [4.0, 1.0],
                        50.0,
                        0.8,
                        0.2,
                        mode,
                        &original,
                        true,
                    )
                    .unwrap(),
                "{mode:?}"
            );
            assert_ne!(doc.layers[doc.active].pixels, original);
            let i = (48 * 96 + 48) * 4;
            scratch
                .liquify(
                    &mut doc,
                    [48.5, 48.5],
                    [0.0, 0.0],
                    50.0,
                    1.0,
                    0.0,
                    Mode::Reconstruct,
                    &original,
                    true,
                )
                .unwrap();
            assert_eq!(
                &doc.layers[doc.active].pixels[i..i + 4],
                &original[i..i + 4]
            );
        }
    }
    #[test]
    fn clone_uses_frozen_source_preserves_selection_and_alpha_lock() {
        let mut doc = gradient();
        let source = doc.layers[doc.active].pixels.clone();
        let mask = Mask::try_rectangle(
            96,
            96,
            Rect::from_min_max(Pos2::new(50.0, 40.0), Pos2::new(65.0, 60.0)),
        )
        .unwrap();
        doc.selection = mask.map(std::sync::Arc::new);
        doc.layers[doc.active].alpha_lock = true;
        let i = (48 * 96 + 55) * 4;
        doc.layers[doc.active].pixels[i + 3] = 100;
        assert!(clone_dab(
            &mut doc,
            &source,
            [55.5, 48.5],
            [20.5, 20.5],
            &brush(30.0)
        ));
        assert_eq!(at(&doc, 55, 48), [40, 40, 120, 100]);
        assert_eq!(at(&doc, 48, 48), [96, 96, 120, 255]);
        // Editing the source in the live document cannot feed back into this stroke.
        doc.layers[doc.active].pixels[(20 * 96 + 20) * 4..(20 * 96 + 20) * 4 + 4].fill(0);
        clone_dab(&mut doc, &source, [55.5, 48.5], [20.5, 20.5], &brush(10.0));
        assert_eq!(at(&doc, 55, 48), [40, 40, 120, 100]);
    }
    #[test]
    fn transparent_interpolation_has_no_black_fringe_and_outside_source_is_empty() {
        let pixels = [255, 20, 10, 255, 0, 0, 0, 0];
        let source = Source {
            pixels: &pixels,
            width: 2,
            height: 1,
            origin: [0, 0],
        };
        assert_eq!(source.sample([1.0, 0.5]), [255, 20, 10, 128]);
        assert_eq!(source.sample([-2.0, 0.5]), [0; 4]);
        let mut doc = gradient();
        let before = doc.layers[doc.active].pixels.clone();
        assert!(!clone_dab(
            &mut doc,
            &before,
            [50.0, 50.0],
            [-100.0, -100.0],
            &brush(10.0)
        ));
        assert_eq!(doc.layers[doc.active].pixels, before);
    }
    #[test]
    fn liquify_respects_mask_and_zero_strength_is_a_noop() {
        let mut doc = gradient();
        let before = doc.layers[doc.active].pixels.clone();
        doc.selection = Mask::try_rectangle(
            96,
            96,
            Rect::from_min_max(Pos2::new(50.0, 40.0), Pos2::new(65.0, 60.0)),
        )
        .unwrap()
        .map(std::sync::Arc::new);
        let mut scratch = Scratch::default();
        assert!(
            !scratch
                .liquify(
                    &mut doc,
                    [48.0, 48.0],
                    [8.0, 0.0],
                    40.0,
                    0.0,
                    0.0,
                    Mode::Push,
                    &before,
                    true,
                )
                .unwrap()
        );
        assert!(doc.damage.is_none());
        assert!(
            scratch
                .liquify(
                    &mut doc,
                    [48.0, 48.0],
                    [8.0, 0.0],
                    40.0,
                    1.0,
                    0.0,
                    Mode::Push,
                    &before,
                    true,
                )
                .unwrap()
        );
        assert_eq!(at(&doc, 48, 48), [96, 96, 120, 255]);
        assert_ne!(at(&doc, 55, 48), [110, 96, 120, 255]);
    }
}
