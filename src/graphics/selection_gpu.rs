//! Composite the floating cut and its background before applying layer opacity.
//! Uses egui's existing GPU textures; moving/resizing uploads no pixels.
use crate::view::CanvasView;
use eframe::{
    egui,
    glow::{self, HasContext},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct Shader {
    gl: Arc<glow::Context>,
    program: glow::Program,
    vao: glow::VertexArray,
    destroyed: AtomicBool,
}
#[derive(Clone, Copy)]
pub struct Preview {
    pub view: CanvasView,
    pub bounds: egui::Rect,
    pub textures: [egui::TextureId; 2],
    pub opacity: f32,
    pub flip: [bool; 2],
    pub angle: f32,
    pub region: egui::Rect,
}
impl Shader {
    pub fn new(gl: Arc<glow::Context>) -> Result<Arc<Self>, String> {
        // Creation and painting are called with the application's GL context current.
        unsafe {
            let version = gl.get_parameter_string(glow::VERSION);
            let header = if version.starts_with("OpenGL ES") {
                "#version 300 es\nprecision highp float;\n"
            } else {
                "#version 330 core\n"
            };
            let vertex = r#"
uniform vec2 u_screen;
uniform vec2 u_doc;
uniform vec4 u_view;
uniform vec4 u_region;
uniform float u_mirror;
out vec2 v_doc;
void main() {
    vec2 uv=vec2(float(gl_VertexID & 1),float((gl_VertexID >> 1) & 1));
    v_doc=u_region.xy+uv*u_region.zw;
    vec2 p=v_doc-u_doc*0.5;
    p.x*=u_mirror;
    float c=cos(u_view.w),s=sin(u_view.w);
    p=mat2(c,s,-s,c)*p*u_view.z+u_view.xy;
    gl_Position=vec4(2.0*p.x/u_screen.x-1.0,1.0-2.0*p.y/u_screen.y,0.0,1.0);
}
"#;
            let fragment = r#"
uniform sampler2D u_base;
uniform sampler2D u_cut;
uniform vec2 u_doc;
uniform vec4 u_bounds;
uniform vec2 u_flip;
uniform float u_angle;
uniform float u_opacity;
in vec2 v_doc;
out vec4 color;
void main() {
    vec4 base=vec4(0.0),cut=vec4(0.0);
    if(all(greaterThanEqual(v_doc,vec2(0.0))) && all(lessThan(v_doc,u_doc)))
        base=texture(u_base,v_doc/u_doc);
    vec2 center=u_bounds.xy+u_bounds.zw*0.5;
    float c=cos(u_angle),s=sin(u_angle);
    vec2 local=mat2(c,-s,s,c)*(v_doc-center)+center;
    vec2 uv=(local-u_bounds.xy)/u_bounds.zw;
    if(all(greaterThanEqual(uv,vec2(0.0))) && all(lessThanEqual(uv,vec2(1.0)))) {
        uv=mix(uv,1.0-uv,u_flip);
        cut=texture(u_cut,uv);
    }
    color=(cut+base*(1.0-cut.a))*u_opacity;
}
"#;
            let program = gl.create_program()?;
            for (kind, source) in [
                (glow::VERTEX_SHADER, vertex),
                (glow::FRAGMENT_SHADER, fragment),
            ] {
                let shader = match gl.create_shader(kind) {
                    Ok(s) => s,
                    Err(e) => {
                        gl.delete_program(program);
                        return Err(e);
                    }
                };
                gl.shader_source(shader, &format!("{header}{source}"));
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    let error = gl.get_shader_info_log(shader);
                    gl.delete_shader(shader);
                    gl.delete_program(program);
                    return Err(error);
                }
                gl.attach_shader(program, shader);
                gl.delete_shader(shader);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                let error = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(error);
            }
            let vao = match gl.create_vertex_array() {
                Ok(v) => v,
                Err(e) => {
                    gl.delete_program(program);
                    return Err(e);
                }
            };
            Ok(Arc::new(Self {
                gl,
                program,
                vao,
                destroyed: AtomicBool::new(false),
            }))
        }
    }
    pub fn callback(self: &Arc<Self>, rect: egui::Rect, preview: Preview) -> egui::PaintCallback {
        let shader = self.clone();
        egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                shader.paint(info, painter, preview);
            })),
        }
    }
    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        painter: &eframe::egui_glow::Painter,
        p: Preview,
    ) {
        if self.destroyed.load(Ordering::Relaxed) {
            return;
        }
        let (Some(base), Some(cut)) = (
            painter.texture(p.textures[0]),
            painter.texture(p.textures[1]),
        ) else {
            return;
        };
        let gl = painter.gl();
        let region = egui::Rect::from_min_size(egui::Pos2::ZERO, p.view.size).union(p.region);
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            let uniform = |name| gl.get_uniform_location(self.program, name);
            gl.uniform_2_f32(
                uniform("u_screen").as_ref(),
                info.viewport.width(),
                info.viewport.height(),
            );
            gl.uniform_2_f32(uniform("u_doc").as_ref(), p.view.size.x, p.view.size.y);
            gl.uniform_4_f32(
                uniform("u_view").as_ref(),
                p.view.center.x - info.viewport.left(),
                p.view.center.y - info.viewport.top(),
                p.view.zoom,
                p.view.angle,
            );
            gl.uniform_4_f32(
                uniform("u_region").as_ref(),
                region.min.x,
                region.min.y,
                region.width(),
                region.height(),
            );
            gl.uniform_4_f32(
                uniform("u_bounds").as_ref(),
                p.bounds.min.x,
                p.bounds.min.y,
                p.bounds.width(),
                p.bounds.height(),
            );
            gl.uniform_1_f32(
                uniform("u_mirror").as_ref(),
                if p.view.mirrored { -1.0 } else { 1.0 },
            );
            gl.uniform_2_f32(
                uniform("u_flip").as_ref(),
                if p.flip[0] { 1.0 } else { 0.0 },
                if p.flip[1] { 1.0 } else { 0.0 },
            );
            gl.uniform_1_f32(uniform("u_angle").as_ref(), p.angle);
            gl.uniform_1_f32(uniform("u_opacity").as_ref(), p.opacity);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(base));
            gl.uniform_1_i32(uniform("u_base").as_ref(), 0);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(cut));
            gl.uniform_1_i32(uniform("u_cut").as_ref(), 1);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            // egui_glow restores the rest of its rendering state after the callback.
        }
    }
    pub fn destroy(&self) {
        if !self.destroyed.swap(true, Ordering::Relaxed) {
            unsafe {
                self.gl.delete_vertex_array(self.vao);
                self.gl.delete_program(self.program);
            }
        }
    }
}
