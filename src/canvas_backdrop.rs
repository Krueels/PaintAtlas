//! The drawing is a full-viewport background; input and editing overlays stay
//! clipped to the docked canvas. Reserve its paint slot before any UI is built.
use super::*;
impl PaintAtlas {
    pub(super) fn canvas_document_shapes(
        &self,
        clip: egui::Rect,
        view: CanvasView,
    ) -> Vec<egui::Shape> {
        let mut shapes = Vec::new();
        if let Some(checker) = &self.checker {
            shapes.push(egui::Shape::mesh(view.mesh(
                checker.id(),
                view.size / 16.0,
                Color32::WHITE,
            )));
        }
        if let Some(texture) = &self.blend_fallback {
            shapes.push(egui::Shape::mesh(view.mesh(
                texture.id(),
                Vec2::splat(1.0),
                Color32::WHITE,
            )));
        } else if let Some(shader) = &self.blend_shader
            && self
                .doc
                .layers
                .iter()
                .any(|l| l.visible && l.blend_mode != crate::blend::BlendMode::Normal)
        {
            let mut layers = Vec::new();
            let floating = self
                .selection
                .floating
                .as_ref()
                .zip(self.selection.textures.as_ref());
            let floating_input =
                |f: &crate::selection::Floating,
                 textures: &(egui::TextureHandle, egui::TextureHandle)| {
                    crate::blend_gpu::Floating {
                        textures: [textures.0.id(), textures.1.id()],
                        bounds: f.bounds,
                        angle: f.angle,
                        flip: [f.flip_x, f.flip_y],
                    }
                };
            for (index, (layer, texture)) in self.doc.layers.iter().zip(&self.textures).enumerate()
            {
                if let Some((f, textures)) = floating
                    && f.new_layer
                    && f.layer == index
                {
                    layers.push(crate::blend_gpu::Layer {
                        texture: textures.0.id(),
                        opacity: 1.0,
                        mode: crate::blend::BlendMode::Normal,
                        floating: Some(floating_input(f, textures)),
                    });
                }
                if layer.visible {
                    layers.push(crate::blend_gpu::Layer {
                        texture: texture.id(),
                        opacity: layer.opacity,
                        mode: layer.blend_mode,
                        floating: floating
                            .filter(|(f, _)| !f.new_layer && f.layer == index)
                            .map(|(f, textures)| floating_input(f, textures)),
                    });
                }
            }
            if let Some((f, textures)) = floating
                && f.new_layer
                && f.layer == self.doc.layers.len()
            {
                layers.push(crate::blend_gpu::Layer {
                    texture: textures.0.id(),
                    opacity: 1.0,
                    mode: crate::blend::BlendMode::Normal,
                    floating: Some(floating_input(f, textures)),
                });
            }
            shapes.push(shader.callback(clip, view, layers).into());
        } else {
            for (index, (layer, texture)) in self.doc.layers.iter().zip(&self.textures).enumerate()
            {
                if let Some(preview) =
                    self.selection_preview_shapes(clip, view, index, Color32::WHITE, true)
                {
                    shapes.extend(preview);
                }
                if layer.visible {
                    let tint = Color32::from_white_alpha((layer.opacity * 255.0).round() as u8);
                    if let Some(preview) =
                        self.selection_preview_shapes(clip, view, index, tint, false)
                    {
                        shapes.extend(preview);
                    } else {
                        shapes.push(egui::Shape::mesh(view.mesh(
                            texture.id(),
                            Vec2::splat(1.0),
                            tint,
                        )));
                    }
                }
            }
            if let Some(preview) = self.selection_preview_shapes(
                clip,
                view,
                self.doc.layers.len(),
                Color32::WHITE,
                true,
            ) {
                shapes.extend(preview);
            }
        }
        let mut border = view.corners().to_vec();
        border.push(border[0]);
        shapes.push(egui::Shape::line(
            border,
            Stroke::new(1.0_f32, Color32::from_gray(70)),
        ));
        shapes
    }
    /// Correct fallback for headless rendering or unavailable shader support.
    /// Native OpenGL rendering never flattens the document on the CPU.
    pub(super) fn fallback_blend_composite(&self) -> Vec<u8> {
        let mut out = vec![0; (self.doc.width * self.doc.height * 4) as usize];
        let transformed = self
            .selection
            .floating
            .as_ref()
            .map(|f| f.rasterize(self.doc.width, self.doc.height).0);
        let draw = |out: &mut [u8], pixels: &[u8], opacity, mode: crate::blend::BlendMode| {
            for (dst, src) in out
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(pixels.as_chunks::<4>().0)
            {
                if src[3] > 0 {
                    mode.composite(dst, *src, opacity);
                }
            }
        };
        for (index, layer) in self.doc.layers.iter().enumerate() {
            if let (Some(f), Some(pixels)) = (&self.selection.floating, &transformed)
                && f.new_layer
                && f.layer == index
            {
                draw(&mut out, pixels, 1.0, crate::blend::BlendMode::Normal);
            }
            if layer.visible {
                let replacement = self
                    .selection
                    .floating
                    .as_ref()
                    .is_some_and(|f| !f.new_layer && f.layer == index);
                let pixels = if replacement {
                    transformed.as_deref().unwrap()
                } else {
                    &layer.pixels
                };
                draw(&mut out, pixels, layer.opacity, layer.blend_mode);
            }
        }
        if let (Some(f), Some(pixels)) = (&self.selection.floating, &transformed)
            && f.new_layer
            && f.layer == self.doc.layers.len()
        {
            draw(&mut out, pixels, 1.0, crate::blend::BlendMode::Normal);
        }
        out
    }
    fn background_view(&self) -> Option<CanvasView> {
        self.last_canvas_bounds.map(|bounds| CanvasView {
            center: bounds.center() + self.pan,
            size: Vec2::new(self.doc.width as f32, self.doc.height as f32),
            zoom: self.zoom,
            angle: self.angle,
            mirrored: self.mirrored,
        })
    }
    pub(super) fn prepare_canvas_backdrop(&mut self, ctx: &egui::Context) {
        let painter = ctx.layer_painter(egui::LayerId::background());
        painter.rect_filled(ctx.content_rect(), 0.0, Color32::from_gray(18));
        let slot = painter.add(egui::Shape::Noop);
        self.canvas_backdrop_slot = Some((painter.clone(), slot));
        self.root_pixels_per_point = ctx.pixels_per_point();
        self.root_window_origin =
            ctx.input(|i| i.viewport().inner_rect.map_or(Pos2::ZERO, |r| r.min));
        if !self.startup_canvas_required {
            self.sync_layer_textures(ctx);
            if let Some(view) = self.background_view() {
                painter.set(
                    slot,
                    egui::Shape::Vec(self.canvas_document_shapes(ctx.content_rect(), view)),
                );
            }
        }
    }
    pub(super) fn paint_canvas_document(
        &mut self,
        ui: &egui::Ui,
        bounds: egui::Rect,
        view: CanvasView,
    ) {
        self.last_canvas_bounds = Some(bounds);
        if let Some((painter, slot)) = &self.canvas_backdrop_slot {
            painter.set(
                *slot,
                egui::Shape::Vec(self.canvas_document_shapes(ui.ctx().content_rect(), view)),
            );
        } else {
            // Headless tests and standalone canvas previews retain their own clipping.
            ui.painter()
                .with_clip_rect(bounds)
                .extend(self.canvas_document_shapes(bounds, view));
        }
    }
    pub(super) fn paint_external_canvas_backdrop(&self, ctx: &egui::Context) {
        let painter = ctx.layer_painter(egui::LayerId::background());
        painter.rect_filled(ctx.content_rect(), 0.0, Color32::from_gray(18));
        if let Some(mut view) = self.background_view() {
            let origin = ctx.input(|i| {
                i.viewport()
                    .inner_rect
                    .map_or(self.root_window_origin, |r| r.min)
            });
            view = external_view(
                view,
                self.root_window_origin,
                self.root_pixels_per_point,
                origin,
                ctx.pixels_per_point(),
            );
            painter.extend(self.canvas_document_shapes(ctx.content_rect(), view));
        }
    }
    pub(super) fn capture_external_glass_smoke(&self, ctx: &egui::Context) {
        if !std::env::var("PAINTATLAS_SMOKE_SCENE").is_ok_and(|s| s == "glass-canvas-native") {
            return;
        }
        let Some(path) = &self.screenshot else {
            return;
        };
        let frame = ctx.data_mut(|data| {
            let id = egui::Id::new("external_glass_smoke_frames");
            let frame = data.get_temp::<u32>(id).unwrap_or(0) + 1;
            data.insert_temp(id, frame);
            frame
        });
        if frame == 4 {
            // eframe's immediate viewports do not return Screenshot events.
            // This readback exists only in the explicit smoke-test scene.
            let path = path.with_file_name("native-panel.png");
            let callback = egui::PaintCallback {
                rect: ctx.content_rect(),
                callback: std::sync::Arc::new(eframe::egui_glow::CallbackFn::new(
                    move |info, painter| {
                        let image = painter.read_screen_rgba(info.screen_size_px);
                        let bytes: Vec<u8> =
                            image.pixels.iter().flat_map(|p| p.to_array()).collect();
                        match image::save_buffer(
                            &path,
                            &bytes,
                            image.width() as u32,
                            image.height() as u32,
                            image::ColorType::Rgba8,
                        ) {
                            Ok(()) => eprintln!("Screenshot nativa: {}", path.display()),
                            Err(e) => eprintln!("Screenshot nativa: {e}"),
                        }
                    },
                )),
            };
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Debug,
                egui::Id::new("native_glass_capture"),
            ))
            .add(callback);
        }
        ctx.request_repaint();
    }
}

fn external_view(
    mut view: CanvasView,
    root_origin: Pos2,
    root_scale: f32,
    origin: Pos2,
    scale: f32,
) -> CanvasView {
    view.center = ((view.center.to_vec2() + root_origin.to_vec2()) * root_scale
        - origin.to_vec2() * scale)
        .to_pos2()
        / scale;
    view.zoom *= root_scale / scale;
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_views_align_canvas_in_physical_pixels_across_dpi_scales() {
        let view = CanvasView {
            center: Pos2::new(200.0, 300.0),
            size: Vec2::new(64.0, 128.0),
            zoom: 4.0,
            angle: 0.4,
            mirrored: true,
        };
        let root = Pos2::new(-100.0, 50.0);
        let external = Pos2::new(300.0, 125.0);
        let mapped = external_view(view, root, 1.5, external, 2.0);
        for point in [[0.0, 0.0], [30.0, 100.0], [64.0, 128.0]] {
            let main_pixel = (view.screen(point).to_vec2() + root.to_vec2()) * 1.5;
            let external_pixel = (mapped.screen(point).to_vec2() + external.to_vec2()) * 2.0;
            assert!((main_pixel - external_pixel).length() < 0.001);
        }
    }
    #[test]
    fn canvas_extends_behind_panel_but_panel_click_cannot_paint() {
        let ctx = egui::Context::default();
        let mut app = PaintAtlas::from_context(&ctx);
        app.reset(Document::new(64, 64, false), None, false);
        app.fit = false;
        app.zoom = 8.0;
        let initial = app.doc.composite();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let draw = |app: &mut PaintAtlas, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.prepare_canvas_backdrop(ctx);
                    egui::SidePanel::left("glass_panel")
                        .exact_width(110.0)
                        .show(ctx, |ui| {
                            ui.label("Painel");
                        });
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| app.canvas_ui(ui));
                },
            )
        };
        let panel_point = Pos2::new(40.0, 200.0);
        let output = draw(
            &mut app,
            vec![
                egui::Event::PointerMoved(panel_point),
                button(panel_point, true),
            ],
        );
        assert!(app.gesture.is_none());
        assert_eq!(app.doc.composite(), initial);
        let bounds = app.last_canvas_bounds.unwrap();
        assert!(output.shapes.iter().any(|s| s.clip_rect.width() == 400.0
            && matches!(&s.shape, egui::Shape::Vec(shapes) if shapes.iter().any(|shape|
                matches!(shape, egui::Shape::Mesh(mesh) if mesh.vertices.iter().any(|v| v.pos.x < bounds.left()))))));
        draw(&mut app, vec![button(panel_point, false)]);
        let center = bounds.center();
        draw(
            &mut app,
            vec![egui::Event::PointerMoved(center), button(center, true)],
        );
        assert!(app.gesture.is_some());
        draw(&mut app, vec![button(center, false)]);
        assert_ne!(app.doc.composite(), initial);
    }
}
