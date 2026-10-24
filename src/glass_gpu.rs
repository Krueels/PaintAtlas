//! GPU-only backdrop blur. The current framebuffer is copied only for the small
//! glass surface being drawn; no CPU readback and no document texture changes.
use eframe::{
    egui,
    glow::{self, HasContext},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
struct Snapshot {
    texture: glow::Texture,
    width: i32,
    height: i32,
    query: Option<glow::Query>,
    pending_query: bool,
}
pub struct Shader {
    gl: Arc<glow::Context>,
    program: glow::Program,
    vao: glow::VertexArray,
    snapshot: Mutex<Snapshot>,
    destroyed: AtomicBool,
}
impl Shader {
    pub fn new(gl: Arc<glow::Context>) -> Result<Arc<Self>, String> {
        unsafe {
            let version = gl.get_parameter_string(glow::VERSION);
            let header = if version.starts_with("OpenGL ES") {
                "#version 300 es\nprecision highp float;\n"
            } else {
                "#version 330 core\n"
            };
            let program = gl.create_program()?;
            let vertex = r#"
out vec2 v_uv;
void main() {
    v_uv=vec2(float(gl_VertexID & 1),float((gl_VertexID >> 1) & 1));
    gl_Position=vec4(v_uv*2.0-1.0,0.0,1.0);
}"#;
            let fragment = r#"
uniform sampler2D u_backdrop;
uniform vec2 u_capacity;
uniform vec2 u_origin;
uniform vec2 u_size;
uniform vec2 u_copy;
uniform float u_blur;
uniform float u_corner;
uniform float u_amount;
in vec2 v_uv;
out vec4 color;
vec3 sample_at(vec2 p) {
    // Keep the surface's own title and controls outside the blur kernel.
    p=clamp(p,max(vec2(0.5),u_origin+vec2(0.5)),min(u_copy-vec2(0.5),u_origin+u_size-vec2(0.5)));
    return texture(u_backdrop,p/u_capacity).rgb;
}
void main() {
    vec2 p=u_origin+v_uv*u_size;
    vec3 rgb=sample_at(p)*0.20;
    for(int i=0;i<8;i++) {
        float angle=float(i)*0.785398163;
        vec2 dir=vec2(cos(angle),sin(angle));
        rgb+=sample_at(p+dir*u_blur*0.7)*0.065;
        rgb+=sample_at(p+dir*u_blur*1.5)*0.035;
    }
    vec2 q=abs((v_uv-0.5)*u_size)-(u_size*0.5-u_corner);
    float distance=length(max(q,vec2(0.0)))+min(max(q.x,q.y),0.0)-u_corner;
    float coverage=(1.0-smoothstep(-0.75,0.75,distance))*u_amount;
    color=vec4(rgb*coverage,coverage);
}"#;
            for (kind, body) in [
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
                gl.shader_source(shader, &format!("{header}{body}"));
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
                let e = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(e);
            }
            let vao = match gl.create_vertex_array() {
                Ok(v) => v,
                Err(e) => {
                    gl.delete_program(program);
                    return Err(e);
                }
            };
            let texture = match gl.create_texture() {
                Ok(t) => t,
                Err(e) => {
                    gl.delete_program(program);
                    gl.delete_vertex_array(vao);
                    return Err(e);
                }
            };
            Ok(Arc::new(Self {
                gl: gl.clone(),
                program,
                vao,
                snapshot: Mutex::new(Snapshot {
                    texture,
                    width: 0,
                    height: 0,
                    query: if std::env::var_os("PAINTATLAS_GLASS_DIAGNOSTICS").is_some() {
                        gl.create_query().ok()
                    } else {
                        None
                    },
                    pending_query: false,
                }),
                destroyed: AtomicBool::new(false),
            }))
        }
    }
    pub fn callback(
        self: &Arc<Self>,
        rect: egui::Rect,
        corner: f32,
        strength: f32,
        opacity: f32,
    ) -> egui::PaintCallback {
        let shader = self.clone();
        egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                shader.paint(info, painter, corner, strength, opacity);
            })),
        }
    }
    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        painter: &eframe::egui_glow::Painter,
        corner: f32,
        strength: f32,
        opacity: f32,
    ) {
        if self.destroyed.load(Ordering::Relaxed) {
            return;
        }
        let gl = painter.gl();
        let Ok(mut snapshot) = self.snapshot.lock() else {
            return;
        };
        let viewport = info.viewport_in_pixels();
        let blur = (12.0 + strength * 12.0) * info.pixels_per_point;
        let pad = (blur * 1.6).ceil() as i32;
        let x = (viewport.left_px - pad).max(0);
        let y = (viewport.from_bottom_px - pad).max(0);
        let right = (viewport.left_px + viewport.width_px + pad).min(info.screen_size_px[0] as i32);
        let top =
            (viewport.from_bottom_px + viewport.height_px + pad).min(info.screen_size_px[1] as i32);
        let width = right - x;
        let height = top - y;
        if width <= 0 || height <= 0 {
            return;
        }
        unsafe {
            // CopyTexSubImage cannot read a multisampled default framebuffer.
            // NativeOptions uses zero samples; retain tinted surfaces if a future backend enables MSAA.
            if gl.get_parameter_i32(glow::SAMPLES) > 1 {
                return;
            }
            if let Some(query) = snapshot.query
                && snapshot.pending_query
                && gl.get_query_parameter_u32(query, glow::QUERY_RESULT_AVAILABLE) != 0
            {
                let nanos = gl.get_query_parameter_u32(query, glow::QUERY_RESULT);
                eprintln!(
                    "Glass GPU: {:.3} ms ({} × {} px)",
                    nanos as f64 / 1_000_000.0,
                    width,
                    height
                );
                snapshot.pending_query = false;
            }
            let measuring = snapshot.query.is_some() && !snapshot.pending_query;
            if measuring {
                gl.begin_query(glow::TIME_ELAPSED, snapshot.query.unwrap());
            }
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(snapshot.texture));
            if width > snapshot.width || height > snapshot.height {
                snapshot.width = snapshot.width.max(width);
                snapshot.height = snapshot.height.max(height);
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    snapshot.width,
                    snapshot.height,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_WRAP_S,
                    glow::CLAMP_TO_EDGE as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_WRAP_T,
                    glow::CLAMP_TO_EDGE as i32,
                );
            }
            gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, x, y, width, height);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            let uniform = |name| gl.get_uniform_location(self.program, name);
            gl.uniform_1_i32(uniform("u_backdrop").as_ref(), 0);
            gl.uniform_2_f32(
                uniform("u_capacity").as_ref(),
                snapshot.width as f32,
                snapshot.height as f32,
            );
            gl.uniform_2_f32(
                uniform("u_origin").as_ref(),
                (viewport.left_px - x) as f32,
                (viewport.from_bottom_px - y) as f32,
            );
            gl.uniform_2_f32(
                uniform("u_size").as_ref(),
                viewport.width_px as f32,
                viewport.height_px as f32,
            );
            gl.uniform_2_f32(uniform("u_copy").as_ref(), width as f32, height as f32);
            gl.uniform_1_f32(uniform("u_blur").as_ref(), blur);
            gl.uniform_1_f32(uniform("u_corner").as_ref(), corner * info.pixels_per_point);
            gl.uniform_1_f32(uniform("u_amount").as_ref(), opacity);
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
            if measuring {
                gl.end_query(glow::TIME_ELAPSED);
                snapshot.pending_query = true;
            }

            gl.bind_vertex_array(None);
            gl.use_program(None);
            // egui_glow restores its renderer state after this callback.
        }
    }
    pub fn destroy(&self) {
        if !self.destroyed.swap(true, Ordering::Relaxed) {
            unsafe {
                if let Ok(snapshot) = self.snapshot.lock() {
                    self.gl.delete_texture(snapshot.texture);
                    if let Some(query) = snapshot.query {
                        self.gl.delete_query(query);
                    }
                }
                self.gl.delete_vertex_array(self.vao);
                self.gl.delete_program(self.program);
            }
        }
    }
}
