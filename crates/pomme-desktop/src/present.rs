//! Putting the phone on screen with wgpu: the picture (premultiplied RGBA)
//! goes up as a texture and a full-window triangle samples it, scaled to fit.
//! Where the platform composites windows with alpha, everything outside the
//! phone is transparent, so the window takes the phone's shape, rounded
//! corners and all, as the iOS Simulator's does. Elsewhere it's the backdrop.

use std::sync::Arc;
use winit::window::Window;

/// The window's background where it can't be transparent (the Simulator's dark backdrop).
pub const BACKDROP: [f32; 3] = [0x1e as f32 / 255.0, 0x1e as f32 / 255.0, 0x20 as f32 / 255.0];

/// How the surface blends with what's behind the window.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Blend {
    /// Colors are premultiplied by alpha (what the phone picture already is).
    Premultiplied,
    /// Colors are straight: the shader divides by alpha.
    Straight,
    /// No transparency: composite over the backdrop.
    Opaque,
}

pub struct Presenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    /// The phone texture, its bind group and size.
    texture: Option<(wgpu::Texture, wgpu::BindGroup, u32, u32)>,
    blend: Blend,
}

const SHADER: &str = r#"
struct U {
    // Window size (px), phone size (px), fit (scale, x, y), blend mode.
    window: vec2<f32>,
    phone: vec2<f32>,
    fit: vec3<f32>,
    mode: f32,
    backdrop: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct V { @builtin(position) pos: vec4<f32>, @location(0) px: vec2<f32> };

@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    // One triangle covering the window.
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var v: V;
    v.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    v.px = uv * u.window;
    return v;
}

@fragment fn fs(v: V) -> @location(0) vec4<f32> {
    let q = (v.px - u.fit.yz) / u.fit.x;
    var c = vec4<f32>(0.0);
    if (q.x >= 0.0 && q.y >= 0.0 && q.x < u.phone.x && q.y < u.phone.y) {
        c = textureSample(tex, samp, q / u.phone);
    }
    if (u.mode == 2.0) {
        // Opaque: the (premultiplied) phone over the backdrop.
        return vec4<f32>(c.rgb + u.backdrop.rgb * (1.0 - c.a), 1.0);
    }
    if (u.mode == 1.0 && c.a > 0.0) {
        return vec4<f32>(c.rgb / c.a, c.a);
    }
    return c;
}
"#;

impl Presenter {
    pub fn new(window: Arc<Window>) -> Result<Presenter, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(window.clone())));
        let surface = instance.create_surface(window.clone()).map_err(|e| format!("no surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("pomme window"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("no GPU device: {e}"))?;
        let caps = surface.get_capabilities(&adapter);
        // Pixels go through as they are: a non-sRGB format if there is one.
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).or_else(|| caps.formats.first().copied()).ok_or("the surface has no formats")?;
        let (alpha_mode, blend) = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            (wgpu::CompositeAlphaMode::PreMultiplied, Blend::Premultiplied)
        } else if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
            (wgpu::CompositeAlphaMode::PostMultiplied, Blend::Straight)
        } else {
            (caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto), Blend::Opaque)
        };
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("phone"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("phone"),
            entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(1, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
                entry(2, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("phone"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("phone"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor { label: Some("phone"), size: 48, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        Ok(Presenter { surface, device, queue, config, pipeline, layout, sampler, uniforms, texture: None, blend })
    }

    /// Whether the window can show through where the phone isn't.
    pub fn transparent(&self) -> bool {
        self.blend != Blend::Opaque
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// Show `rgba` (premultiplied, `w` × `h`), placed by `fit` (scale, x, y in window pixels).
    pub fn present(&mut self, rgba: &[u8], w: u32, h: u32, fit: (f32, f32, f32)) {
        if self.texture.as_ref().is_none_or(|t| (t.2, t.3) != (w, h)) {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("phone"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("phone"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniforms.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.texture = Some((texture, group, w, h));
        }
        let (texture, group, ..) = self.texture.as_ref().unwrap();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let mode = match self.blend {
            Blend::Premultiplied => 0.0,
            Blend::Straight => 1.0,
            Blend::Opaque => 2.0,
        };
        let u: [f32; 12] = [
            self.config.width as f32, self.config.height as f32, w as f32, h as f32,
            fit.0, fit.1, fit.2, mode,
            BACKDROP[0], BACKDROP[1], BACKDROP[2], 1.0,
        ];
        let bytes: Vec<u8> = u.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.queue.write_buffer(&self.uniforms, 0, &bytes);
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                drop(f);
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("phone"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }
}
