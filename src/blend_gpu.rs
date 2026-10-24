//! Composite visible layer textures in two reusable, viewport-sized targets.
//! No canvas readback, CPU flattening, or document-sized upload while drawing.
use crate::{blend::BlendMode, view::CanvasView};
use eframe::{
    egui,
    glow::{self, HasContext},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy)]
pub struct Floating {
    pub textures: [egui::TextureId; 2],
    pub bounds: egui::Rect,
    pub angle: f32,
    pub flip: [bool; 2],
}
#[derive(Clone, Copy)]
pub struct Layer {
    pub texture: egui::TextureId,
    pub opacity: f32,
    pub mode: BlendMode,
    pub floating: Option<Floating>,
}
struct Targets {
    size: [i32; 2],
    textures: [glow::Texture; 2],
    fbos: [glow::Framebuffer; 2],
}
pub struct Shader {
    gl: Arc<glow::Context>,
    program: glow::Program,
    vao: glow::VertexArray,
    targets: Mutex<Vec<Targets>>,
    destroyed: AtomicBool,
}
const VERTEX: &str = r#"
out vec2 v_uv;
void main() {
    v_uv=vec2(float(gl_VertexID & 1),float((gl_VertexID >> 1) & 1));
    gl_Position=vec4(v_uv*2.0-1.0,0.0,1.0);
}
"#;
const FRAGMENT: &str = r#"
in vec2 v_uv;
out vec4 color;
uniform sampler2D u_previous, u_layer, u_cut;
uniform int u_final, u_mode, u_floating;
uniform vec2 u_doc, u_center, u_origin, u_extent, u_flip;
uniform vec4 u_bounds;
uniform float u_zoom, u_view_angle, u_mirror, u_opacity, u_angle;
float lum(vec3 c) { return dot(c,vec3(0.3,0.59,0.11)); }
float sat(vec3 c) { return max(max(c.r,c.g),c.b)-min(min(c.r,c.g),c.b); }
vec3 set_sat(vec3 c,float s) {
    float range=sat(c),n=min(min(c.r,c.g),c.b);
    return range>0.0?(c-vec3(n))*s/range:vec3(0.0);
}
vec3 set_lum(vec3 c,float target) {
    c+=vec3(target-lum(c));
    float l=lum(c),n=min(min(c.r,c.g),c.b),x=max(max(c.r,c.g),c.b);
    if(n<0.0) c=vec3(l)+(c-vec3(l))*l/(l-n);
    if(x>1.0) c=vec3(l)+(c-vec3(l))*(1.0-l)/(x-l);
    return clamp(c,0.0,1.0);
}
float channel(float b,float s) {
    if(u_mode==1) return b*s;
    if(u_mode==2) return b+s-b*s;
    if(u_mode==3) return b<=0.5?2.0*b*s:1.0-2.0*(1.0-b)*(1.0-s);
    if(u_mode==4) {
        float d=b<=0.25?((16.0*b-12.0)*b+4.0)*b:sqrt(b);
        return s<=0.5?b-(1.0-2.0*s)*b*(1.0-b):b+(2.0*s-1.0)*(d-b);
    }
    if(u_mode==5) return s<=0.5?2.0*b*s:1.0-2.0*(1.0-b)*(1.0-s);
    if(u_mode==6) return min(b,s);
    if(u_mode==7) return max(b,s);
    if(u_mode==8) return b==0.0?0.0:(s==1.0?1.0:min(1.0,b/(1.0-s)));
    if(u_mode==9) return b==1.0?1.0:(s==0.0?0.0:1.0-min(1.0,(1.0-b)/s));
    if(u_mode==10) return abs(b-s);
    if(u_mode==11) return b+s-2.0*b*s;
    if(u_mode==12) return min(1.0,b+s);
    if(u_mode==13) return max(0.0,b-s);
    return s;
}
vec3 blend_rgb(vec3 b,vec3 s) {
    if(u_mode==14) return set_lum(set_sat(s,sat(b)),lum(b));
    if(u_mode==15) return set_lum(set_sat(b,sat(s)),lum(b));
    if(u_mode==16) return set_lum(s,lum(b));
    if(u_mode==17) return set_lum(b,lum(s));
    return vec3(channel(b.r,s.r),channel(b.g,s.g),channel(b.b,s.b));
}
void main() {
    vec4 base=texture(u_previous,v_uv);
    if(u_final!=0) { color=base; return; }
    vec2 screen=u_origin+vec2(v_uv.x,1.0-v_uv.y)*u_extent;
    vec2 d=(screen-u_center)/u_zoom;
    float c=cos(u_view_angle),s=sin(u_view_angle);
    d=mat2(c,-s,s,c)*d;
    d.x*=u_mirror;
    d+=u_doc*0.5;
    vec4 source=vec4(0.0);
    if(all(greaterThanEqual(d,vec2(0.0))) && all(lessThan(d,u_doc))) source=texture(u_layer,d/u_doc);
    if(u_floating!=0) {
        vec2 center=u_bounds.xy+u_bounds.zw*0.5;
        c=cos(u_angle); s=sin(u_angle);
        vec2 local=mat2(c,-s,s,c)*(d-center)+center;
        vec2 uv=(local-u_bounds.xy)/u_bounds.zw;
        if(all(greaterThanEqual(uv,vec2(0.0))) && all(lessThanEqual(uv,vec2(1.0)))) {
            vec4 cut=texture(u_cut,mix(uv,1.0-uv,u_flip));
            source=cut+source*(1.0-cut.a);
        }
    }
    source*=u_opacity;
    if(source.a<=0.0) { color=base; return; }
    if(u_mode==0) { color=source+base*(1.0-source.a); return; }
    vec3 b=base.a>0.0?base.rgb/base.a:vec3(0.0);
    vec3 t=source.a>0.0?source.rgb/source.a:vec3(0.0);
    vec3 rgb=(1.0-source.a)*base.rgb+(1.0-base.a)*source.rgb+base.a*source.a*blend_rgb(b,t);
    color=vec4(rgb,source.a+base.a*(1.0-source.a));
}
"#;
impl Shader {
    pub fn new(gl: Arc<glow::Context>) -> Result<Arc<Self>, String> {
        unsafe {
            let header = if gl
                .get_parameter_string(glow::VERSION)
                .starts_with("OpenGL ES")
            {
                "#version 300 es\nprecision highp float;\nprecision highp int;\n"
            } else {
                "#version 330 core\n"
            };
            let program = gl.create_program()?;
            for (kind, source) in [
                (glow::VERTEX_SHADER, VERTEX),
                (glow::FRAGMENT_SHADER, FRAGMENT),
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
                targets: Mutex::new(Vec::new()),
                destroyed: AtomicBool::new(false),
            }))
        }
    }
    pub fn callback(
        self: &Arc<Self>,
        rect: egui::Rect,
        view: CanvasView,
        layers: Vec<Layer>,
    ) -> egui::PaintCallback {
        let shader = self.clone();
        egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                shader.paint(info, painter, view, &layers);
            })),
        }
    }
    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        painter: &eframe::egui_glow::Painter,
        view: CanvasView,
        layers: &[Layer],
    ) {
        if self.destroyed.load(Ordering::Relaxed) {
            return;
        }
        let vp = info.viewport_in_pixels();
        let size = [vp.width_px, vp.height_px];
        if size[0] <= 0 || size[1] <= 0 {
            return;
        }
        let gl = &self.gl;
        let mut pool = self.targets.lock().unwrap();
        unsafe {
            let original = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            let mut scissor = [0; 4];
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor);
            let scissor_enabled = gl.is_enabled(glow::SCISSOR_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            let index = if let Some(i) = pool.iter().position(|t| t.size == size) {
                i
            } else {
                // Bound the pool while sharing resources across native viewports.
                if pool.len() >= 4 {
                    let old = pool.remove(0);
                    old.destroy(gl);
                }
                match Targets::new(gl, size) {
                    Ok(t) => pool.push(t),
                    Err(e) => {
                        eprintln!("Mesclagem OpenGL: {e}");
                        gl.bind_framebuffer(glow::FRAMEBUFFER, original);
                        if scissor_enabled {
                            gl.enable(glow::SCISSOR_TEST);
                        }
                        return;
                    }
                }
                pool.len() - 1
            };
            let target = &pool[index];
            gl.viewport(0, 0, size[0], size[1]);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbos[0]));
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            let uniform = |name| gl.get_uniform_location(self.program, name);
            gl.uniform_1_i32(uniform("u_final").as_ref(), 0);
            gl.uniform_1_i32(uniform("u_previous").as_ref(), 0);
            gl.uniform_1_i32(uniform("u_layer").as_ref(), 1);
            gl.uniform_1_i32(uniform("u_cut").as_ref(), 2);
            gl.uniform_2_f32(uniform("u_doc").as_ref(), view.size.x, view.size.y);
            gl.uniform_2_f32(uniform("u_center").as_ref(), view.center.x, view.center.y);
            gl.uniform_2_f32(
                uniform("u_origin").as_ref(),
                info.viewport.min.x,
                info.viewport.min.y,
            );
            gl.uniform_2_f32(
                uniform("u_extent").as_ref(),
                info.viewport.width(),
                info.viewport.height(),
            );
            gl.uniform_1_f32(uniform("u_zoom").as_ref(), view.zoom);
            gl.uniform_1_f32(uniform("u_view_angle").as_ref(), view.angle);
            gl.uniform_1_f32(
                uniform("u_mirror").as_ref(),
                if view.mirrored { -1.0 } else { 1.0 },
            );
            let mut current = 0;
            for layer in layers {
                if layer.opacity <= 0.0 {
                    continue;
                }
                let texture = layer.floating.map_or(layer.texture, |f| f.textures[0]);
                let Some(texture) = painter.texture(texture) else {
                    continue;
                };
                let next = 1 - current;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbos[next]));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(target.textures[current]));
                gl.active_texture(glow::TEXTURE1);
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.uniform_1_i32(uniform("u_mode").as_ref(), layer.mode as i32);
                gl.uniform_1_f32(uniform("u_opacity").as_ref(), layer.opacity);
                gl.uniform_1_i32(
                    uniform("u_floating").as_ref(),
                    i32::from(layer.floating.is_some()),
                );
                if let Some(f) = layer.floating {
                    gl.active_texture(glow::TEXTURE2);
                    gl.bind_texture(glow::TEXTURE_2D, painter.texture(f.textures[1]));
                    gl.uniform_4_f32(
                        uniform("u_bounds").as_ref(),
                        f.bounds.min.x,
                        f.bounds.min.y,
                        f.bounds.width(),
                        f.bounds.height(),
                    );
                    gl.uniform_1_f32(uniform("u_angle").as_ref(), f.angle);
                    gl.uniform_2_f32(
                        uniform("u_flip").as_ref(),
                        f32::from(f.flip[0]),
                        f32::from(f.flip[1]),
                    );
                }
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                current = next;
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, original);
            gl.viewport(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
            gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
            if scissor_enabled {
                gl.enable(glow::SCISSOR_TEST);
            }
            gl.uniform_1_i32(uniform("u_final").as_ref(), 1);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(target.textures[current]));
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            for unit in [glow::TEXTURE2, glow::TEXTURE1, glow::TEXTURE0] {
                gl.active_texture(unit);
                gl.bind_texture(glow::TEXTURE_2D, None);
            }
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }
    pub fn destroy(&self) {
        if !self.destroyed.swap(true, Ordering::Relaxed) {
            unsafe {
                for t in self.targets.lock().unwrap().drain(..) {
                    t.destroy(&self.gl);
                }
                self.gl.delete_vertex_array(self.vao);
                self.gl.delete_program(self.program);
            }
        }
    }
}
impl Targets {
    unsafe fn new(gl: &glow::Context, size: [i32; 2]) -> Result<Self, String> {
        unsafe {
            let mut textures = Vec::new();
            let mut fbos = Vec::new();
            let result = (|| {
                for _ in 0..2 {
                    let texture = gl.create_texture()?;
                    textures.push(texture);
                    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::NEAREST as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAG_FILTER,
                        glow::NEAREST as i32,
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
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA8 as i32,
                        size[0],
                        size[1],
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(None),
                    );
                    let fbo = gl.create_framebuffer()?;
                    fbos.push(fbo);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                    gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::TEXTURE_2D,
                        Some(texture),
                        0,
                    );
                    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE
                    {
                        return Err("Framebuffer incompleto".into());
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                for t in textures {
                    gl.delete_texture(t);
                }
                for f in fbos {
                    gl.delete_framebuffer(f);
                }
                return Err(e);
            }
            Ok(Self {
                size,
                textures: [textures[0], textures[1]],
                fbos: [fbos[0], fbos[1]],
            })
        }
    }
    unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            for f in self.fbos {
                gl.delete_framebuffer(f);
            }
            for t in self.textures {
                gl.delete_texture(t);
            }
        }
    }
}
