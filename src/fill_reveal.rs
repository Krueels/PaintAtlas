//! Reveal the actual filled pixels radially, before layer opacity and blending.
//! Pixel/history changes commit once; subsequent frames animate only GPU uniforms.
use super::*;

const DURATION: f32 = 0.58;

pub(super) struct Reveal {
    layer: usize,
    before: egui::TextureHandle,
    point: [f32; 2],
    radius: f32,
    elapsed: f32,
    first_frame: bool,
}
#[derive(Default)]
pub(super) struct State {
    reveal: Option<Reveal>,
}
impl State {
    pub fn clear(&mut self) {
        self.reveal = None;
    }
    pub fn active(&self) -> bool {
        self.reveal.is_some()
    }
    pub fn advance(&mut self, ctx: &egui::Context) {
        if crate::theme::reduced_motion(ctx) {
            self.clear();
            return;
        }
        if let Some(reveal) = &mut self.reveal {
            if reveal.first_frame {
                reveal.first_frame = false;
            } else {
                reveal.elapsed += ctx.input(|i| i.stable_dt).clamp(0.0, 1.0 / 30.0);
            }
            if reveal.elapsed >= DURATION {
                self.clear();
            }
            ctx.request_repaint();
        }
    }
    pub fn preview(&self, layer: usize) -> Option<crate::blend_gpu::Reveal> {
        self.reveal.as_ref().filter(|r| r.layer == layer).map(|r| {
            let t = (r.elapsed / DURATION).clamp(0.0, 1.0);
            let smooth = t * t * (3.0 - 2.0 * t);
            crate::blend_gpu::Reveal {
                before: r.before.id(),
                point: r.point,
                radius: r.radius * smooth,
            }
        })
    }
}

fn changed_radius(before: &[u8], after: &[u8], width: u32, point: [f32; 2]) -> Option<f32> {
    let mut squared: f32 = -1.0;
    for (i, (a, b)) in before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0)
        .enumerate()
    {
        if a != b {
            let dx = (i % width as usize) as f32 + 0.5 - point[0];
            let dy = (i / width as usize) as f32 + 0.5 - point[1];
            squared = squared.max(dx * dx + dy * dy);
        }
    }
    (squared >= 0.0).then(|| squared.sqrt() + 2.0)
}
impl PaintAtlas {
    pub(super) fn animated_fill(
        &mut self,
        ctx: &egui::Context,
        point: [f32; 2],
        color: [u8; 4],
        opacity: f32,
    ) {
        self.fill_reveal.clear();
        let mut root = self.doc.active;
        while let Some(parent) = self.doc.layers[root].parent {
            let Some(index) = self
                .doc
                .layers
                .iter()
                .position(|layer| layer.group == Some(parent))
            else {
                break;
            };
            root = index;
        }
        let before = (!crate::theme::reduced_motion(ctx) && self.blend_shader.is_some())
            .then(|| self.doc.layers[root].pixels.clone());
        self.doc.fill(point, color, opacity, self.tolerance);
        self.changed();
        if let Some(before) = before
            && let Some(radius) = changed_radius(
                &before,
                &self.doc.layers[root].pixels,
                self.doc.width,
                point,
            )
        {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [self.doc.width as usize, self.doc.height as usize],
                &before,
            );
            self.fill_reveal.reveal = Some(Reveal {
                layer: root,
                before: ctx.load_texture("fill-before", image, egui::TextureOptions::NEAREST),
                point,
                radius,
                elapsed: 0.0,
                first_frame: true,
            });
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fill_result_matches_unanimated_fill_and_history_keeps_one_action() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(32, 32, false), None, false);
        let mask = std::sync::Arc::new(
            crate::selection::Mask::rectangle(
                32,
                32,
                egui::Rect::from_min_max(Pos2::new(4.0, 5.0), Pos2::new(20.0, 23.0)),
            )
            .unwrap(),
        );
        app.doc.selection = Some(mask);
        app.tolerance = 10;
        let original = app.doc.layers[0].pixels.clone();
        let mut reference = app.doc.clone();
        reference.fill([8.0, 9.0], [80, 120, 200, 255], 0.5, 10);
        app.checkpoint();
        app.animated_fill(&ctx, [8.0, 9.0], [80, 120, 200, 255], 0.5);
        assert_eq!(app.doc.layers[0].pixels, reference.layers[0].pixels);
        app.history.commit(&app.doc);
        assert_eq!(app.history.counts().0, 1);
        app.undo();
        assert_eq!(app.doc.layers[0].pixels, original);
        app.redo();
        assert_eq!(app.doc.layers[0].pixels, reference.layers[0].pixels);
    }
    #[test]
    fn radius_covers_only_actual_changes_and_noop_has_no_animation() {
        let before = vec![0; 16 * 16 * 4];
        let mut after = before.clone();
        assert!(changed_radius(&before, &after, 16, [2.0, 3.0]).is_none());
        after[(3 * 16 + 2) * 4] = 255;
        let r = changed_radius(&before, &after, 16, [2.0, 3.0]).unwrap();
        assert!(r < 3.0);
        after[(15 * 16 + 15) * 4] = 255;
        assert!(changed_radius(&before, &after, 16, [2.0, 3.0]).unwrap() > 20.0);
    }
    #[test]
    fn reveal_starts_at_zero_survives_stalls_and_finishes_exactly() {
        let ctx = egui::Context::default();
        let mut state = State {
            reveal: Some(Reveal {
                layer: 2,
                before: ctx.load_texture(
                    "before-test",
                    egui::ColorImage::filled([2, 2], Color32::BLACK),
                    egui::TextureOptions::NEAREST,
                ),
                point: [1.0, 1.0],
                radius: 100.0,
                elapsed: 0.0,
                first_frame: true,
            }),
        };
        assert_eq!(state.preview(2).unwrap().radius, 0.0);
        assert!(state.preview(1).is_none());
        let frame = |state: &mut State, time| {
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| state.advance(ctx),
            );
        };
        frame(&mut state, 0.0);
        assert_eq!(state.preview(2).unwrap().radius, 0.0);
        frame(&mut state, 5.0);
        let radius = state.preview(2).unwrap().radius;
        assert!(radius > 0.0 && radius < 5.0);
        for i in 0..90 {
            frame(&mut state, 5.0 + (i + 1) as f64 / 60.0);
        }
        assert!(!state.active());
    }
}
