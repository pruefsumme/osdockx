use crate::config::RenderMode;
use crate::layout::{DockLayout, Point};
use crate::model::DockModel;
use crate::shelf::{ShelfRenderResult, ShelfRenderer};
use crate::theme::{Color, Theme};
use glow::HasContext;
use gtk::GLArea;
use gtk::prelude::*;
use libloading::Library;
use std::ffi::{CString, c_uchar, c_void};

type GlGetProcAddress = unsafe extern "C" fn(*const c_uchar) -> *const c_void;

pub struct Scene3dRenderer {
    gl_scene: Option<GlScene>,
    fallback_reason: Option<String>,
    size: (i32, i32),
    scale_factor: f64,
}

struct GlScene {
    gl: glow::Context,
    _loader: GlLoader,
    program: glow::Program,
    vertex_buffer: glow::Buffer,
    vertex_array: Option<glow::VertexArray>,
    color_uniform: Option<glow::UniformLocation>,
}

struct GlLoader {
    _library: Library,
    get_proc_address: GlGetProcAddress,
}

impl Scene3dRenderer {
    pub fn new() -> Self {
        Self {
            gl_scene: None,
            fallback_reason: None,
            size: (1, 1),
            scale_factor: 1.0,
        }
    }

    pub fn render_gl_area(
        &mut self,
        area: &GLArea,
        layout: &DockLayout,
        model: &DockModel,
        theme: &Theme,
        hover: Option<Point>,
    ) -> bool {
        if let Some(error) = area.error() {
            self.fallback_reason = Some(format!("GLArea error: {error}"));
            return false;
        }

        area.make_current();
        if self.gl_scene.is_none() {
            match GlScene::new() {
                Ok(scene) => {
                    self.fallback_reason = None;
                    self.gl_scene = Some(scene);
                }
                Err(error) => {
                    self.fallback_reason = Some(error);
                    return false;
                }
            }
        }

        let size = (area.width().max(1), area.height().max(1));
        self.size = size;
        let Some(scene) = self.gl_scene.as_mut() else {
            return false;
        };
        scene.render(size, layout, model, theme, hover);
        true
    }
}

impl Default for Scene3dRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ShelfRenderer for Scene3dRenderer {
    fn kind(&self) -> RenderMode {
        RenderMode::Scene3d
    }

    fn resize(&mut self, size: (i32, i32), scale_factor: f64) {
        self.size = size;
        self.scale_factor = scale_factor;
    }

    fn render_shelf(
        &mut self,
        _layout: &DockLayout,
        _model: &DockModel,
        _theme: &Theme,
        _hover: Option<Point>,
    ) -> ShelfRenderResult {
        if let Some(reason) = self.fallback_reason.clone() {
            ShelfRenderResult::fallback(reason)
        } else {
            ShelfRenderResult::rendered()
        }
    }

    fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }
}

impl GlScene {
    fn new() -> Result<Self, String> {
        let loader = GlLoader::new().map_err(|error| format!("could not load OpenGL: {error}"))?;
        let gl = unsafe { glow::Context::from_loader_function(|name| loader.load(name)) };
        let program = create_program(&gl)?;
        let vertex_buffer = unsafe {
            gl.create_buffer()
                .map_err(|error| format!("could not create GL vertex buffer: {error}"))?
        };
        let vertex_array = unsafe { gl.create_vertex_array().ok() };
        let color_uniform = unsafe { gl.get_uniform_location(program, "u_color") };
        unsafe {
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
        }
        Ok(Self {
            gl,
            _loader: loader,
            program,
            vertex_buffer,
            vertex_array,
            color_uniform,
        })
    }

    fn render(
        &mut self,
        size: (i32, i32),
        layout: &DockLayout,
        model: &DockModel,
        theme: &Theme,
        hover: Option<Point>,
    ) {
        unsafe {
            self.gl.viewport(0, 0, size.0, size.1);
            self.gl.clear_color(0.0, 0.0, 0.0, 0.0);
            self.gl
                .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            self.gl.use_program(Some(self.program));
            if let Some(vertex_array) = self.vertex_array {
                self.gl.bind_vertex_array(Some(vertex_array));
            }
            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
            self.gl.enable_vertex_attrib_array(0);
            self.gl
                .vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
        }

        draw_scene(self, size, layout, model, theme, hover);

        unsafe {
            self.gl.disable_vertex_attrib_array(0);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, None);
            if self.vertex_array.is_some() {
                self.gl.bind_vertex_array(None);
            }
            self.gl.use_program(None);
        }
    }

    fn draw_polygon(&self, size: (i32, i32), points: &[Point], color: Color) {
        if points.len() < 3 || color.alpha <= 0.0 {
            return;
        }
        let vertices = points
            .iter()
            .flat_map(|point| {
                let x = point.x as f32 / size.0.max(1) as f32 * 2.0 - 1.0;
                let y = 1.0 - point.y as f32 / size.1.max(1) as f32 * 2.0;
                [x, y]
            })
            .collect::<Vec<_>>();

        unsafe {
            if let Some(location) = self.color_uniform.as_ref() {
                self.gl.uniform_4_f32(
                    Some(location),
                    color.red as f32,
                    color.green as f32,
                    color.blue as f32,
                    color.alpha as f32,
                );
            }
            self.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                f32_bytes(&vertices),
                glow::STREAM_DRAW,
            );
            self.gl
                .draw_arrays(glow::TRIANGLE_FAN, 0, points.len() as i32);
        }
    }
}

impl Drop for GlScene {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_program(self.program);
            self.gl.delete_buffer(self.vertex_buffer);
            if let Some(vertex_array) = self.vertex_array {
                self.gl.delete_vertex_array(vertex_array);
            }
        }
    }
}

impl GlLoader {
    fn new() -> Result<Self, libloading::Error> {
        let library = unsafe { Library::new("libGL.so.1")? };
        let get_proc_address =
            unsafe { *library.get::<GlGetProcAddress>(b"glXGetProcAddressARB\0")? };
        Ok(Self {
            _library: library,
            get_proc_address,
        })
    }

    fn load(&self, name: &str) -> *const c_void {
        let Ok(name) = CString::new(name) else {
            return std::ptr::null();
        };
        unsafe { (self.get_proc_address)(name.as_ptr().cast::<c_uchar>()) }
    }
}

fn create_program(gl: &glow::Context) -> Result<glow::Program, String> {
    let vertex_shader = compile_shader(gl, glow::VERTEX_SHADER, VERTEX_SHADER)?;
    let fragment_shader = compile_shader(gl, glow::FRAGMENT_SHADER, FRAGMENT_SHADER)?;
    let program = unsafe {
        gl.create_program()
            .map_err(|error| format!("could not create GL program: {error}"))?
    };
    unsafe {
        gl.attach_shader(program, vertex_shader);
        gl.attach_shader(program, fragment_shader);
        gl.link_program(program);
        gl.detach_shader(program, vertex_shader);
        gl.detach_shader(program, fragment_shader);
        gl.delete_shader(vertex_shader);
        gl.delete_shader(fragment_shader);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(format!("could not link GL program: {log}"));
        }
    }
    Ok(program)
}

fn compile_shader(gl: &glow::Context, kind: u32, source: &str) -> Result<glow::Shader, String> {
    let shader = unsafe {
        gl.create_shader(kind)
            .map_err(|error| format!("could not create GL shader: {error}"))?
    };
    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("could not compile GL shader: {log}"));
        }
    }
    Ok(shader)
}

fn draw_scene(
    scene: &GlScene,
    size: (i32, i32),
    layout: &DockLayout,
    model: &DockModel,
    theme: &Theme,
    hover: Option<Point>,
) {
    let shelf = layout.shelf;
    let slant = shelf.height * theme.shelf_slant_ratio;
    let top = shelf.y;
    let horizon = shelf.y + shelf.height * (0.34 + theme.tilt * 0.12).clamp(0.32, 0.56);
    let bottom = shelf.y + shelf.height;
    let bevel = shelf.height * theme.bevel;

    for icon in &layout.icons {
        let item = &model.items[icon.item_index];
        let active_boost = if item.active { 1.2 } else { 1.0 };
        let shadow_alpha = 0.22 * theme.shadow_strength * icon.scale * active_boost;
        draw_ellipse(
            scene,
            size,
            Point {
                x: icon.rect.center_x(),
                y: horizon + shelf.height * 0.18,
            },
            icon.rect.width * 0.46,
            shelf.height * 0.13,
            Color::rgba(0.0, 0.0, 0.0, shadow_alpha),
        );
    }

    scene.draw_polygon(
        size,
        &[
            Point {
                x: shelf.x + slant,
                y: top,
            },
            Point {
                x: shelf.x + shelf.width - slant,
                y: top,
            },
            Point {
                x: shelf.x + shelf.width - slant * 0.45,
                y: horizon,
            },
            Point {
                x: shelf.x + slant * 0.45,
                y: horizon,
            },
        ],
        theme
            .shelf_top
            .mix(theme.shelf_bottom, theme.material_roughness * 0.24)
            .with_alpha(theme.floor_opacity),
    );

    for icon in &layout.icons {
        let reflection_alpha = theme.reflection_opacity * 0.62 * icon.scale.min(1.45);
        let reflected_height = icon.rect.height * theme.reflection_height * 0.62;
        scene.draw_polygon(
            size,
            &[
                Point {
                    x: icon.rect.x + icon.rect.width * 0.18,
                    y: horizon - 1.0,
                },
                Point {
                    x: icon.rect.x + icon.rect.width * 0.82,
                    y: horizon - 1.0,
                },
                Point {
                    x: icon.rect.x + icon.rect.width * 0.68,
                    y: (horizon + reflected_height).min(bottom - 2.0),
                },
                Point {
                    x: icon.rect.x + icon.rect.width * 0.32,
                    y: (horizon + reflected_height).min(bottom - 2.0),
                },
            ],
            theme
                .shelf_highlight
                .with_alpha(reflection_alpha * (1.0 - theme.reflection_blur * 0.55)),
        );
    }

    scene.draw_polygon(
        size,
        &[
            Point {
                x: shelf.x + slant * 0.45,
                y: horizon,
            },
            Point {
                x: shelf.x + shelf.width - slant * 0.45,
                y: horizon,
            },
            Point {
                x: shelf.x + shelf.width,
                y: bottom,
            },
            Point {
                x: shelf.x,
                y: bottom,
            },
        ],
        theme
            .shelf_bottom
            .mix(
                Color::rgba(0.08, 0.11, 0.15, 1.0),
                0.35 + theme.depth * 0.18,
            )
            .with_alpha(0.86),
    );

    scene.draw_polygon(
        size,
        &[
            Point {
                x: shelf.x + slant,
                y: top,
            },
            Point {
                x: shelf.x + shelf.width - slant,
                y: top,
            },
            Point {
                x: shelf.x + shelf.width - slant + bevel,
                y: top + bevel.max(1.0),
            },
            Point {
                x: shelf.x + slant - bevel,
                y: top + bevel.max(1.0),
            },
        ],
        theme
            .shelf_highlight
            .with_alpha(0.22 + theme.highlight_strength * 0.48),
    );

    scene.draw_polygon(
        size,
        &[
            Point {
                x: shelf.x + 3.0,
                y: bottom - 1.5,
            },
            Point {
                x: shelf.x + shelf.width - 3.0,
                y: bottom - 1.5,
            },
            Point {
                x: shelf.x + shelf.width - 5.0,
                y: bottom,
            },
            Point {
                x: shelf.x + 5.0,
                y: bottom,
            },
        ],
        Color::rgba(0.0, 0.0, 0.0, 0.22 + theme.depth * 0.18),
    );

    if let Some(point) = hover {
        draw_ellipse(
            scene,
            size,
            Point {
                x: point.x,
                y: horizon,
            },
            shelf.height * 1.15,
            shelf.height * 0.16,
            theme
                .shelf_highlight
                .with_alpha(0.12 * theme.highlight_strength),
        );
    }
}

fn draw_ellipse(
    scene: &GlScene,
    size: (i32, i32),
    center: Point,
    radius_x: f64,
    radius_y: f64,
    color: Color,
) {
    let mut points = Vec::with_capacity(34);
    points.push(center);
    for index in 0..=32 {
        let angle = index as f64 / 32.0 * std::f64::consts::TAU;
        points.push(Point {
            x: center.x + angle.cos() * radius_x,
            y: center.y + angle.sin() * radius_y,
        });
    }
    scene.draw_polygon(size, &points, color);
}

fn f32_bytes(values: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

const VERTEX_SHADER: &str = r#"#version 330 core
layout(location = 0) in vec2 a_pos;
void main() {
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const FRAGMENT_SHADER: &str = r#"#version 330 core
uniform vec4 u_color;
out vec4 frag_color;
void main() {
    frag_color = u_color;
}
"#;

trait ColorMix {
    fn mix(self, other: Color, amount: f64) -> Color;
}

impl ColorMix for Color {
    fn mix(self, other: Color, amount: f64) -> Color {
        let amount = amount.clamp(0.0, 1.0);
        Color::rgba(
            self.red + (other.red - self.red) * amount,
            self.green + (other.green - self.green) * amount,
            self.blue + (other.blue - self.blue) * amount,
            self.alpha + (other.alpha - self.alpha) * amount,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_bytes_preserves_length() {
        let values = [0.0_f32, 1.0, -1.0, 0.5];
        assert_eq!(f32_bytes(&values).len(), values.len() * 4);
    }

    #[test]
    fn color_mix_interpolates_channels() {
        let mixed = Color::rgba(0.0, 0.0, 0.0, 1.0).mix(Color::rgba(1.0, 0.5, 0.0, 0.5), 0.5);
        assert_eq!(mixed, Color::rgba(0.5, 0.25, 0.0, 0.75));
    }
}
