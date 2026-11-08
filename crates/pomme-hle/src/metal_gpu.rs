//! Running recorded Metal commands on wgpu.
//!
//! At `commit` the command buffer's commands run in order:
//! - buffer → buffer copies happen in guest memory (where Metal buffers live);
//! - a render pass mirrors the Metal buffers its draws read (from guest memory,
//!   at that moment) into one upload, copies each draw's uniform slots into
//!   256-byte-aligned scratch, and draws with pipelines made on first use for
//!   each (pipeline, depth-stencil state, topology, cull, winding);
//! - presented drawables are read back into their CAMetalLayer's contents.
//!
//! Shaders come from pomme-msl (MSL → WGSL); its reflection gives each
//! entry point's bindings, so bind group layouts are explicit.

use crate::metal::{self, Attachment, BufRef, Cmd, DepthStencilDesc, DrawCall, Obj, Pass, PipelineDesc, SamplerDesc, StencilDesc};
use crate::Emu;
use pomme_msl::{EntryPoint, Stage};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;

type R<T> = std::result::Result<T, String>;

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub adapter_name: String,
    textures: HashMap<u64, Tex>,
    /// Library → its translation and module (or why it failed).
    modules: HashMap<u64, Arc<R<Module>>>,
    pipelines: HashMap<PipelineKey, Arc<R<Pipeline>>>,
    samplers: HashMap<u64, wgpu::Sampler>,
    /// A 1×1 transparent texture (stand-in for unemulated framebuffer fetch).
    blank: (wgpu::Texture, wgpu::TextureView),
    /// Render passes run so far.
    passes: u64,
    /// Errors already printed (each once).
    reported: std::collections::HashSet<String>,
    /// Validation errors wgpu reported (drained after each submit).
    errors: Arc<std::sync::Mutex<Vec<String>>>,
}

struct Tex {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
}

struct Module {
    module: wgpu::ShaderModule,
    entry_points: Vec<EntryPoint>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PipelineKey {
    pipeline: u64,
    depth_stencil: u64,
    topology: u64,
    cull: u64,
    winding: u64,
}

struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    groups: [wgpu::BindGroupLayout; 2],
    /// Per stage: the entry point's bindings.
    entries: [Option<EntryPoint>; 2],
    /// Metal buffer index of each wgpu vertex buffer slot.
    vertex_slots: Vec<u32>,
}

/// wgpu needs uniform offsets aligned to this.
const UNIFORM_ALIGN: usize = 256;

impl Gpu {
    fn new() -> R<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let software = std::env::var("POMME_SOFTWARE_GPU").is_ok();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: software,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("pomme metal"),
            required_features: adapter.features() & wgpu::Features::DEPTH32FLOAT_STENCIL8,
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("no GPU device: {e}"))?;
        let errors = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = errors.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| sink.lock().unwrap().push(e.to_string())));
        let blank_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blank"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let blank = (blank_tex.clone(), blank_tex.create_view(&Default::default()));
        Ok(Gpu { device, queue, adapter_name, blank, passes: 0, reported: Default::default(), textures: HashMap::new(), modules: HashMap::new(), pipelines: HashMap::new(), samplers: HashMap::new(), errors })
    }

    fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut *self.errors.lock().unwrap())
    }
}

/// The GPU, started on first use.
fn gpu(emu: &mut Emu) -> R<Gpu> {
    if let Some(g) = emu.metal.gpu.take() {
        return Ok(g);
    }
    if let Some(e) = &emu.metal.gpu_error {
        return Err(e.clone());
    }
    let g = Gpu::new()?;
    if emu.trace || std::env::var("POMME_TRACE_METAL").is_ok() {
        println!("[metal] running on {}", g.adapter_name);
    }
    Ok(g)
}

// ---- formats ----

fn texture_format(f: u64) -> R<wgpu::TextureFormat> {
    use wgpu::TextureFormat as F;
    Ok(match f {
        1 | 10 => F::R8Unorm, // A8 too: Skia reads coverage from .x
        13 => F::R8Uint,
        25 => F::R16Float,
        30 => F::Rg8Unorm,
        65 => F::Rg16Float,
        70 => F::Rgba8Unorm,
        71 => F::Rgba8UnormSrgb,
        80 => F::Bgra8Unorm,
        81 => F::Bgra8UnormSrgb,
        90 => F::Rgb10a2Unorm,
        115 => F::Rgba16Float,
        252 => F::Depth32Float,
        253 => F::Stencil8,
        260 => F::Depth32FloatStencil8,
        other => return Err(format!("unsupported MTLPixelFormat {other}")),
    })
}

fn vertex_format(f: u64) -> R<wgpu::VertexFormat> {
    use wgpu::VertexFormat as V;
    Ok(match f {
        1 => V::Uint8x2,
        3 => V::Uint8x4,
        4 => V::Sint8x2,
        6 => V::Sint8x4,
        7 => V::Unorm8x2,
        9 => V::Unorm8x4,
        10 => V::Snorm8x2,
        12 => V::Snorm8x4,
        13 => V::Uint16x2,
        15 => V::Uint16x4,
        16 => V::Sint16x2,
        18 => V::Sint16x4,
        19 => V::Unorm16x2,
        21 => V::Unorm16x4,
        22 => V::Snorm16x2,
        24 => V::Snorm16x4,
        25 => V::Float16x2,
        27 => V::Float16x4,
        28 => V::Float32,
        29 => V::Float32x2,
        30 => V::Float32x3,
        31 => V::Float32x4,
        32 => V::Sint32,
        33 => V::Sint32x2,
        34 => V::Sint32x3,
        35 => V::Sint32x4,
        36 => V::Uint32,
        37 => V::Uint32x2,
        38 => V::Uint32x3,
        39 => V::Uint32x4,
        42 => V::Unorm8x4Bgra,
        45 => V::Uint8,
        46 => V::Sint8,
        47 => V::Unorm8,
        48 => V::Snorm8,
        49 => V::Uint16,
        50 => V::Sint16,
        51 => V::Unorm16,
        52 => V::Snorm16,
        53 => V::Float16,
        other => return Err(format!("unsupported MTLVertexFormat {other}")),
    })
}

fn blend_factor(f: u64) -> wgpu::BlendFactor {
    use wgpu::BlendFactor as B;
    match f {
        0 => B::Zero,
        1 => B::One,
        2 => B::Src,
        3 => B::OneMinusSrc,
        4 => B::SrcAlpha,
        5 => B::OneMinusSrcAlpha,
        6 => B::Dst,
        7 => B::OneMinusDst,
        8 => B::DstAlpha,
        9 => B::OneMinusDstAlpha,
        10 => B::SrcAlphaSaturated,
        11 | 13 => B::Constant,
        12 | 14 => B::OneMinusConstant,
        15 => B::Src1,
        16 => B::OneMinusSrc1,
        17 => B::Src1Alpha,
        _ => B::OneMinusSrc1Alpha,
    }
}

fn blend_op(o: u64) -> wgpu::BlendOperation {
    use wgpu::BlendOperation as O;
    match o {
        1 => O::Subtract,
        2 => O::ReverseSubtract,
        3 => O::Min,
        4 => O::Max,
        _ => O::Add,
    }
}

fn compare(c: u64) -> wgpu::CompareFunction {
    use wgpu::CompareFunction as C;
    match c {
        0 => C::Never,
        1 => C::Less,
        2 => C::Equal,
        3 => C::LessEqual,
        4 => C::Greater,
        5 => C::NotEqual,
        6 => C::GreaterEqual,
        _ => C::Always,
    }
}

fn stencil_op(o: u64) -> wgpu::StencilOperation {
    use wgpu::StencilOperation as S;
    match o {
        1 => S::Zero,
        2 => S::Replace,
        3 => S::IncrementClamp,
        4 => S::DecrementClamp,
        5 => S::Invert,
        6 => S::IncrementWrap,
        7 => S::DecrementWrap,
        _ => S::Keep,
    }
}

fn stencil_face(s: &Option<StencilDesc>) -> wgpu::StencilFaceState {
    match s {
        Some(s) => wgpu::StencilFaceState { compare: compare(s.compare), fail_op: stencil_op(s.ops[0]), depth_fail_op: stencil_op(s.ops[1]), pass_op: stencil_op(s.ops[2]) },
        None => wgpu::StencilFaceState::IGNORE,
    }
}

fn topology(p: u64) -> wgpu::PrimitiveTopology {
    use wgpu::PrimitiveTopology as T;
    match p {
        0 => T::PointList,
        1 => T::LineList,
        2 => T::LineStrip,
        4 => T::TriangleStrip,
        _ => T::TriangleList,
    }
}

fn address(a: u64) -> wgpu::AddressMode {
    match a {
        2 => wgpu::AddressMode::Repeat,
        3 => wgpu::AddressMode::MirrorRepeat,
        4 | 5 => wgpu::AddressMode::ClampToBorder,
        _ => wgpu::AddressMode::ClampToEdge,
    }
}

fn filter(f: u64) -> wgpu::FilterMode {
    if f == 1 { wgpu::FilterMode::Linear } else { wgpu::FilterMode::Nearest }
}

// ---- resources ----

fn texture<'a>(g: &'a mut Gpu, emu: &Emu, t: u64) -> R<&'a Tex> {
    if !g.textures.contains_key(&t) {
        let Some(Obj::Texture(desc)) = emu.metal.objects.get(&t) else { return Err(format!("{t:#x} is not a texture")) };
        let format = texture_format(desc.format)?;
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST;
        if !format.is_depth_stencil_format() {
            usage |= wgpu::TextureUsages::TEXTURE_BINDING;
        }
        let texture = g.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: desc.width.max(1) as u32, height: desc.height.max(1) as u32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: desc.samples.max(1) as u32,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        if std::env::var("POMME_TRACE_METAL").is_ok() {
            println!("[metal]  gpu texture {t:#x}: {}×{} {format:?} ×{}", desc.width, desc.height, desc.samples);
        }
        g.textures.insert(t, Tex { texture, view, format });
    }
    Ok(&g.textures[&t])
}

fn sampler<'a>(g: &'a mut Gpu, emu: &Emu, s: u64) -> &'a wgpu::Sampler {
    if !g.samplers.contains_key(&s) {
        let d = match emu.metal.objects.get(&s) {
            Some(Obj::Sampler(d)) => d.clone(),
            _ => SamplerDesc::default(),
        };
        let smp = g.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: address(d.address[0]),
            address_mode_v: address(d.address[1]),
            mag_filter: filter(d.mag),
            min_filter: filter(d.min),
            mipmap_filter: if d.mip == 2 { wgpu::MipmapFilterMode::Linear } else { wgpu::MipmapFilterMode::Nearest },
            border_color: d.address.iter().any(|&a| a >= 4).then_some(wgpu::SamplerBorderColor::TransparentBlack),
            ..Default::default()
        });
        g.samplers.insert(s, smp);
    }
    &g.samplers[&s]
}

fn module(g: &mut Gpu, emu: &Emu, library: u64) -> Arc<R<Module>> {
    if let Some(m) = g.modules.get(&library) {
        return m.clone();
    }
    let m = (|| {
        let Some(Obj::Library { source, constants }) = emu.metal.objects.get(&library) else { return Err("not a library".to_string()) };
        let t = pomme_msl::translate_specialized(source, &constants.iter().copied().collect())?;
        if let Ok(dir) = std::env::var("POMME_DUMP_MSL") {
            let _ = std::fs::write(format!("{dir}/library-{library:x}.wgsl"), &t.wgsl);
        }
        let module = g.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(t.wgsl.into()) });
        Ok(Module { module, entry_points: t.entry_points })
    })();
    let m = Arc::new(m);
    g.modules.insert(library, m.clone());
    m
}

fn bind_layout(g: &Gpu, ep: Option<&EntryPoint>, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayout {
    let mut entries = Vec::new();
    if let Some(ep) = ep {
        for b in &ep.buffers {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: b.binding,
                visibility,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            });
        }
        for t in &ep.textures {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: t.binding,
                visibility,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            });
        }
        for s in &ep.samplers {
            entries.push(wgpu::BindGroupLayoutEntry { binding: s.binding, visibility, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None });
        }
        for &n in &ep.framebuffer_inputs {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: pomme_msl::FRAMEBUFFER_BASE + n,
                visibility,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            });
        }
    }
    g.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries })
}

fn pipeline(g: &mut Gpu, emu: &Emu, key: &PipelineKey, targets: &[Option<wgpu::TextureFormat>], depth_format: Option<wgpu::TextureFormat>, samples: u32) -> Arc<R<Pipeline>> {
    if let Some(p) = g.pipelines.get(key) {
        return p.clone();
    }
    let p = (|| {
        let Some(Obj::Pipeline(desc)) = emu.metal.objects.get(&key.pipeline) else { return Err("no pipeline".to_string()) };
        let desc: &PipelineDesc = desc;
        let ds = match emu.metal.objects.get(&key.depth_stencil) {
            Some(Obj::DepthStencil(d)) => d.clone(),
            _ => DepthStencilDesc { depth_compare: 7, ..Default::default() },
        };
        let (vlib, vname) = desc.vertex.clone().ok_or("no vertex function")?;
        let vm = module(g, emu, vlib);
        let vm = vm.as_ref().as_ref().map_err(|e| format!("vertex shader: {e}"))?;
        let vep = vm.entry_points.iter().find(|e| e.name == vname && e.stage == Stage::Vertex).cloned().ok_or(format!("no vertex entry {vname}"))?;
        let fm = desc.fragment.clone().map(|(lib, name)| (module(g, emu, lib), name));
        let fep = match &fm {
            Some((m, name)) => {
                let m = m.as_ref().as_ref().map_err(|e| format!("fragment shader: {e}"))?;
                Some(m.entry_points.iter().find(|e| &e.name == name && e.stage == Stage::Fragment).cloned().ok_or(format!("no fragment entry {name}"))?)
            }
            None => None,
        };
        let groups = [bind_layout(g, Some(&vep), wgpu::ShaderStages::VERTEX), bind_layout(g, fep.as_ref(), wgpu::ShaderStages::FRAGMENT)];
        let layout = g.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&groups[0]), Some(&groups[1])], immediate_size: 0 });

        // Vertex buffers: one wgpu slot per Metal layout the attributes use.
        let mut vertex_slots = Vec::new();
        let mut attrs_per_slot: Vec<Vec<wgpu::VertexAttribute>> = Vec::new();
        let mut layouts = Vec::new();
        for &(index, stride, step, _) in &desc.layouts {
            let attrs: Vec<_> = desc
                .attributes
                .iter()
                .filter(|a| a.3 == index && vep.inputs.iter().any(|i| i.location == a.0))
                .map(|&(loc, fmt, off, _)| vertex_format(fmt).map(|format| wgpu::VertexAttribute { format, offset: off, shader_location: loc }))
                .collect::<R<_>>()?;
            if attrs.is_empty() {
                continue;
            }
            vertex_slots.push(index);
            attrs_per_slot.push(attrs);
            layouts.push((stride, if step == 2 { wgpu::VertexStepMode::Instance } else { wgpu::VertexStepMode::Vertex }));
        }
        let buffers: Vec<Option<wgpu::VertexBufferLayout>> =
            layouts.iter().zip(&attrs_per_slot).map(|(&(stride, step_mode), attrs)| Some(wgpu::VertexBufferLayout { array_stride: stride, step_mode, attributes: attrs })).collect();

        let color_targets: Vec<Option<wgpu::ColorTargetState>> = targets
            .iter()
            .enumerate()
            .map(|(i, f)| {
                f.map(|format| {
                    let c = desc.colors.iter().find(|c| c.index == i as u32).cloned().unwrap_or_default();
                    let m = c.write_mask;
                    let mut writes = wgpu::ColorWrites::empty();
                    writes.set(wgpu::ColorWrites::RED, m & 8 != 0);
                    writes.set(wgpu::ColorWrites::GREEN, m & 4 != 0);
                    writes.set(wgpu::ColorWrites::BLUE, m & 2 != 0);
                    writes.set(wgpu::ColorWrites::ALPHA, m & 1 != 0);
                    let b = c.blend;
                    wgpu::ColorTargetState {
                        format,
                        blend: c.blending.then(|| wgpu::BlendState {
                            color: wgpu::BlendComponent { src_factor: blend_factor(b[0]), dst_factor: blend_factor(b[1]), operation: blend_op(b[2]) },
                            alpha: wgpu::BlendComponent { src_factor: blend_factor(b[3]), dst_factor: blend_factor(b[4]), operation: blend_op(b[5]) },
                        }),
                        write_mask: writes,
                    }
                })
            })
            .collect();
        let depth_stencil = depth_format.map(|format| {
            let has_depth = format.has_depth_aspect();
            let mask = ds.front.as_ref().or(ds.back.as_ref());
            wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(has_depth && ds.depth_write),
                depth_compare: Some(if has_depth { compare(ds.depth_compare) } else { wgpu::CompareFunction::Always }),
                stencil: wgpu::StencilState {
                    front: stencil_face(&ds.front),
                    back: stencil_face(&ds.back),
                    read_mask: mask.map_or(0xff, |s| s.read_mask),
                    write_mask: mask.map_or(0, |s| s.write_mask),
                },
                bias: wgpu::DepthBiasState::default(),
            }
        });
        let fragment_module = fm.as_ref().map(|(m, _)| m.clone());
        let fragment = match (&fragment_module, &fep) {
            (Some(m), Some(ep)) => {
                let m = m.as_ref().as_ref().map_err(|e| e.clone())?;
                Some(wgpu::FragmentState { module: &m.module, entry_point: Some(&ep.name), compilation_options: Default::default(), targets: &color_targets })
            }
            _ => None,
        };
        let pipeline = g.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &vm.module, entry_point: Some(&vep.name), compilation_options: Default::default(), buffers: &buffers },
            primitive: wgpu::PrimitiveState {
                topology: topology(key.topology),
                strip_index_format: None,
                front_face: if key.winding == 1 { wgpu::FrontFace::Ccw } else { wgpu::FrontFace::Cw },
                cull_mode: match key.cull {
                    1 => Some(wgpu::Face::Front),
                    2 => Some(wgpu::Face::Back),
                    _ => None,
                },
                ..Default::default()
            },
            depth_stencil,
            multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
            fragment,
            multiview_mask: None,
            cache: None,
        });
        Ok(Pipeline { pipeline, groups, entries: [Some(vep), fep], vertex_slots })
    })();
    let p = Arc::new(p);
    g.pipelines.insert(key.clone(), p.clone());
    p
}

// ---- execution ----


/// Bytes of a buffer slot, from its offset to the end of the Metal buffer (at most `max`).
fn slot_bytes(emu: &Emu, r: &BufRef, max: u64) -> Vec<u8> {
    match r {
        BufRef::Buffer(b, off) => metal::buffer_bytes(emu, *b, *off, max).unwrap_or_default(),
        BufRef::Bytes(v) => v.iter().copied().take(max as usize).collect(),
    }
}

fn align(v: &mut Vec<u8>, a: usize) -> usize {
    let at = v.len().next_multiple_of(a);
    v.resize(at, 0);
    at
}

/// One draw, made ready: its pipeline, bind groups and buffer ranges in the pass upload.
struct Prepared {
    pipeline: Arc<R<Pipeline>>,
    /// Per stage: (binding, offset in the upload, size).
    uniforms: [Vec<(u32, u64, u64)>; 2],
    /// (wgpu slot, offset in the upload).
    vertex: Vec<(u32, u64)>,
    /// (offset in the upload, format).
    index: Option<(u64, wgpu::IndexFormat)>,
}

fn run_pass(g: &mut Gpu, emu: &Emu, encoder: &mut wgpu::CommandEncoder, pass: &Pass) -> R<()> {
    // Formats of the attachments (pipelines must match them).
    let mut targets: Vec<Option<wgpu::TextureFormat>> = Vec::new();
    let mut samples = 1;
    for (i, a) in &pass.colors {
        let t = texture(g, emu, a.texture)?;
        let i = *i as usize;
        if targets.len() <= i {
            targets.resize(i + 1, None);
        }
        targets[i] = Some(t.format);
        if a.resolve != 0 {
            texture(g, emu, a.resolve)?;
        }
        if let Some(Obj::Texture(d)) = emu.metal.objects.get(&a.texture) {
            samples = d.samples.max(1) as u32;
        }
    }
    let ds_tex = pass.stencil.as_ref().or(pass.depth.as_ref()).map(|a| a.texture);
    let depth_format = match ds_tex {
        Some(t) => Some(texture(g, emu, t)?.format),
        None => None,
    };

    // Everything the draws read, in one upload.
    let mut upload: Vec<u8> = Vec::new();
    let mut mirrored: HashMap<(u64, u64), u64> = HashMap::new();
    let mut prepared = Vec::new();
    for d in &pass.draws {
        let key = PipelineKey { pipeline: d.state.pipeline, depth_stencil: d.state.depth_stencil, topology: d.primitive, cull: d.state.cull, winding: d.state.winding };
        let p = pipeline(g, emu, &key, &targets, depth_format, samples);
        let pl = match p.as_ref() {
            Ok(pl) => pl,
            Err(e) => {
                if g.reported.insert(e.clone()) {
                    println!("[metal] draw skipped: {e}");
                }
                prepared.push(None);
                continue;
            }
        };
        if std::env::var("POMME_TRACE_METAL").is_ok() {
            println!("[metal]   draw {:?} prim {} slots {:?} scissor {:?} viewport {:?} colors {:?}", d.call, d.primitive, pl.vertex_slots, d.state.scissor, d.state.viewport, match emu.metal.objects.get(&d.state.pipeline) { Some(Obj::Pipeline(p)) => format!("{:?} attrs {:?} layouts {:?}", p.colors, p.attributes, p.layouts), _ => String::new() });
        }
        let mut prep = Prepared { pipeline: p.clone(), uniforms: [Vec::new(), Vec::new()], vertex: Vec::new(), index: None };
        for (stage, ep) in pl.entries.iter().enumerate() {
            let Some(ep) = ep else { continue };
            for b in &ep.buffers {
                let size = (b.size as u64).div_ceil(16).max(1) * 16;
                let at = align(&mut upload, UNIFORM_ALIGN);
                if let Some(r) = d.state.buffers[stage].get(&b.index) {
                    upload.extend(slot_bytes(emu, r, size));
                }
                upload.resize(at + size as usize, 0);
                if std::env::var("POMME_TRACE_METAL").is_ok() {
                    let f: Vec<f32> = upload[at..at + 16].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
                    println!("[metal]     stage {stage} buffer {}: {f:?}…", b.index);
                }
                prep.uniforms[stage].push((b.binding, at as u64, size));
            }
        }
        for (slot, index) in pl.vertex_slots.iter().enumerate() {
            let Some(r) = d.state.buffers[0].get(index) else { continue };
            let at = match r {
                BufRef::Buffer(b, off) => {
                    let base = *mirrored.entry((*b, 0)).or_insert_with(|| {
                        let at = align(&mut upload, UNIFORM_ALIGN);
                        upload.extend(metal::buffer_bytes(emu, *b, 0, u64::MAX).unwrap_or_default());
                        at as u64
                    });
                    base + off
                }
                BufRef::Bytes(v) => {
                    let at = align(&mut upload, 16);
                    upload.extend_from_slice(v);
                    at as u64
                }
            };
            prep.vertex.push((slot as u32, at));
        }
        if let DrawCall::Indexed { buffer, offset, index_type, .. } = &d.call {
            let base = *mirrored.entry((*buffer, 0)).or_insert_with(|| {
                let at = align(&mut upload, UNIFORM_ALIGN);
                upload.extend(metal::buffer_bytes(emu, *buffer, 0, u64::MAX).unwrap_or_default());
                at as u64
            });
            prep.index = Some((base + offset, if *index_type == 1 { wgpu::IndexFormat::Uint32 } else { wgpu::IndexFormat::Uint16 }));
        }
        prepared.push(Some(prep));
    }
    align(&mut upload, 16);
    upload.resize(upload.len().max(16), 0);
    let buf = g.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pass upload"),
        contents: &upload,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::INDEX,
    });

    // Bind groups (textures and samplers made before the pass borrows them).
    for d in &pass.draws {
        for stage in 0..2 {
            for &t in d.state.textures[stage].values() {
                let _ = texture(g, emu, t);
            }
            for &s in d.state.samplers[stage].values() {
                let _ = sampler(g, emu, s);
            }
        }
    }
    let mut groups: Vec<Option<[wgpu::BindGroup; 2]>> = Vec::new();
    for (d, prep) in pass.draws.iter().zip(&prepared) {
        let Some(prep) = prep else {
            groups.push(None);
            continue;
        };
        let Ok(pl) = prep.pipeline.as_ref() else {
            groups.push(None);
            continue;
        };
        let mut made = Vec::new();
        for stage in 0..2 {
            let mut entries = Vec::new();
            for &(binding, offset, size) in &prep.uniforms[stage] {
                entries.push(wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buf, offset, size: std::num::NonZeroU64::new(size) }) });
            }
            if let Some(ep) = &pl.entries[stage] {
                for t in &ep.textures {
                    let Some(tex) = d.state.textures[stage].get(&t.index).and_then(|tid| g.textures.get(tid)) else { return Err(format!("texture slot {} unbound", t.index)) };
                    entries.push(wgpu::BindGroupEntry { binding: t.binding, resource: wgpu::BindingResource::TextureView(&tex.view) });
                }
                for s in &ep.samplers {
                    let Some(smp) = d.state.samplers[stage].get(&s.index).and_then(|sid| g.samplers.get(sid)) else { return Err(format!("sampler slot {} unbound", s.index)) };
                    entries.push(wgpu::BindGroupEntry { binding: s.binding, resource: wgpu::BindingResource::Sampler(smp) });
                }
                // Framebuffer fetch isn't emulated yet (it needs a copy of the attachment
                // mid-pass): those inputs read a transparent texel.
                for &n in &ep.framebuffer_inputs {
                    entries.push(wgpu::BindGroupEntry { binding: pomme_msl::FRAMEBUFFER_BASE + n, resource: wgpu::BindingResource::TextureView(&g.blank.1) });
                }
            }
            made.push(g.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &pl.groups[stage], entries: &entries }));
        }
        let [a, b]: [wgpu::BindGroup; 2] = made.try_into().map_err(|_| "bind groups".to_string())?;
        groups.push(Some([a, b]));
    }

    // The pass itself.
    let load = |a: &Attachment| a.load;
    let color_views: Vec<Option<(&wgpu::TextureView, Option<&wgpu::TextureView>, &Attachment)>> = {
        let mut v = vec![None; targets.len()];
        for (i, a) in &pass.colors {
            v[*i as usize] = Some((&g.textures[&a.texture].view, (a.resolve != 0).then(|| g.textures.get(&a.resolve).map(|t| &t.view)).flatten(), a));
        }
        v
    };
    let colors: Vec<Option<wgpu::RenderPassColorAttachment>> = color_views
        .iter()
        .map(|c| {
            c.map(|(view, resolve, a)| wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: resolve,
                ops: wgpu::Operations {
                    load: if load(a) == 1 {
                        wgpu::LoadOp::Load
                    } else {
                        let c = if load(a) == 2 { a.clear_color } else { [0.0; 4] };
                        wgpu::LoadOp::Clear(wgpu::Color { r: c[0], g: c[1], b: c[2], a: c[3] })
                    },
                    store: if a.store == 0 && a.resolve != 0 { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                },
            })
        })
        .collect();
    let ds_att = pass.stencil.as_ref().or(pass.depth.as_ref());
    let depth_stencil_attachment = ds_att.map(|a| {
        let t = &g.textures[&a.texture];
        let stencil_ops = t.format.has_stencil_aspect().then(|| {
            let s = pass.stencil.as_ref().unwrap_or(a);
            wgpu::Operations { load: if s.load == 1 { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(s.clear_stencil) }, store: wgpu::StoreOp::Store }
        });
        let depth_ops = t.format.has_depth_aspect().then(|| {
            let d = pass.depth.as_ref().unwrap_or(a);
            wgpu::Operations { load: if d.load == 1 { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(d.clear_depth as f32) }, store: wgpu::StoreOp::Store }
        });
        wgpu::RenderPassDepthStencilAttachment { view: &t.view, depth_ops, stencil_ops }
    });
    let (tw, th) = pass.colors.first().and_then(|(_, a)| match emu.metal.objects.get(&a.texture) {
        Some(Obj::Texture(t)) => Some((t.width as u32, t.height as u32)),
        _ => None,
    }).unwrap_or((1, 1));
    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &colors,
        depth_stencil_attachment,
        occlusion_query_set: None,
        timestamp_writes: None,
        multiview_mask: None,
    });
    for ((d, prep), bg) in pass.draws.iter().zip(&prepared).zip(&groups) {
        let (Some(prep), Some(bg)) = (prep, bg) else { continue };
        let Ok(pl) = prep.pipeline.as_ref() else { continue };
        rp.set_pipeline(&pl.pipeline);
        rp.set_bind_group(0, &bg[0], &[]);
        rp.set_bind_group(1, &bg[1], &[]);
        for &(slot, at) in &prep.vertex {
            rp.set_vertex_buffer(slot, buf.slice(at..));
        }
        rp.set_stencil_reference(d.state.stencil_ref[0]);
        let c = d.state.blend_color;
        rp.set_blend_constant(wgpu::Color { r: c[0], g: c[1], b: c[2], a: c[3] });
        match d.state.scissor {
            Some([x, y, w, h]) => {
                let (x, y) = ((x as u32).min(tw), (y as u32).min(th));
                rp.set_scissor_rect(x, y, (w as u32).min(tw - x), (h as u32).min(th - y));
            }
            None => rp.set_scissor_rect(0, 0, tw, th),
        }
        if let Some([x, y, w, h, n, f]) = d.state.viewport {
            rp.set_viewport(x as f32, y as f32, w as f32, h as f32, n as f32, f as f32);
        }
        match &d.call {
            DrawCall::Arrays { start, count, instances, base_instance } => {
                rp.draw(*start as u32..(*start + *count) as u32, *base_instance as u32..(*base_instance + *instances) as u32);
            }
            DrawCall::Indexed { count, instances, base_vertex, base_instance, .. } => {
                if let Some((at, fmt)) = prep.index {
                    rp.set_index_buffer(buf.slice(at..), fmt);
                    rp.draw_indexed(0..*count as u32, *base_vertex as i32, *base_instance as u32..(*base_instance + *instances) as u32);
                }
            }
        }
    }
    drop(rp);
    Ok(())
}

/// Copy the bytes of a texture region back (blocking).
fn read_texture(g: &mut Gpu, emu: &Emu, t: u64) -> R<(Vec<u8>, u32, u32)> {
    texture(g, emu, t)?;
    let tex = &g.textures[&t];
    let (w, h) = (tex.texture.width(), tex.texture.height());
    let bpp = tex.format.block_copy_size(None).unwrap_or(4);
    let row = (w * bpp).next_multiple_of(256);
    let staging = g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = g.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &tex.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &staging, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    g.queue.submit(Some(enc.finish()));
    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    g.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
    let data = slice.get_mapped_range().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity((w * h * bpp) as usize);
    for y in 0..h {
        let s = (y * row) as usize;
        out.extend_from_slice(&data[s..s + (w * bpp) as usize]);
    }
    drop(data);
    staging.unmap();
    Ok((out, w, h))
}

/// Run a committed command buffer and present its drawables.
pub(crate) fn execute(emu: &mut Emu, cmds: &[Cmd], present: &[u64]) -> R<()> {
    let mut g = gpu(emu)?;
    let r = run(&mut g, emu, cmds, present);
    let errors = g.take_errors();
    emu.metal.gpu = Some(g);
    r?;
    if let Some(e) = errors.first() {
        return Err(format!("wgpu: {e}"));
    }
    Ok(())
}

fn run(g: &mut Gpu, emu: &mut Emu, cmds: &[Cmd], present: &[u64]) -> R<()> {
    for c in cmds {
        if std::env::var("POMME_TRACE_METAL").is_ok() {
            match c {
                Cmd::Pass(p) => println!("[metal]  cmd: pass, {} draws", p.draws.len()),
                Cmd::CopyBuffer { src, src_offset, dst, dst_offset, size } => println!("[metal]  cmd: copy {size} bytes {src:#x}+{src_offset} → {dst:#x}+{dst_offset}"),
                other => println!("[metal]  cmd: {other:?}"),
            }
        }
        match c {
            Cmd::CopyBuffer { src, src_offset, dst, dst_offset, size } => {
                metal::copy_buffer(emu, *src, *src_offset, *dst, *dst_offset, *size).map_err(|e| e.to_string())?;
            }
            Cmd::Pass(p) => {
                let mut enc = g.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                run_pass(g, emu, &mut enc, p)?;
                g.queue.submit(Some(enc.finish()));
                g.passes += 1;
                if std::env::var("POMME_TRACE_METAL").is_ok() {
                    let _ = g.device.poll(wgpu::PollType::wait_indefinitely());
                    for e in g.errors.lock().unwrap().iter() {
                        println!("[metal]  error after pass {}: {e}", g.passes);
                    }
                }
                // Debugging aid: POMME_DUMP_PASSES=dir writes every pass's color attachments.
                if let Ok(dir) = std::env::var("POMME_DUMP_PASSES") {
                    for (i, a) in &p.colors {
                        dump(g, emu, a.texture, &format!("{dir}/pass-{:03}-{i}.png", g.passes))?;
                    }
                }
            }
            Cmd::BufferToTexture { buffer, offset, bytes_per_row, texture: t, origin, size } => {
                let rows = metal::buffer_bytes(emu, *buffer, *offset, bytes_per_row * size[1]).unwrap_or_default();
                write(g, emu, *t, *origin, *size, *bytes_per_row, &rows)?;
            }
            Cmd::TextureToTexture { src, src_origin, size, dst, dst_origin } => {
                texture(g, emu, *src)?;
                texture(g, emu, *dst)?;
                let mut enc = g.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                enc.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo { texture: &g.textures[src].texture, mip_level: 0, origin: wgpu::Origin3d { x: src_origin[0] as u32, y: src_origin[1] as u32, z: 0 }, aspect: wgpu::TextureAspect::All },
                    wgpu::TexelCopyTextureInfo { texture: &g.textures[dst].texture, mip_level: 0, origin: wgpu::Origin3d { x: dst_origin[0] as u32, y: dst_origin[1] as u32, z: 0 }, aspect: wgpu::TextureAspect::All },
                    wgpu::Extent3d { width: size[0] as u32, height: size[1] as u32, depth_or_array_layers: 1 },
                );
                g.queue.submit(Some(enc.finish()));
            }
        }
    }
    for &d in present {
        let Some(Obj::Drawable { layer, texture: t }) = emu.metal.objects.get(&d).cloned() else { continue };
        let format = texture(g, emu, t)?.format;
        let (mut px, w, h) = read_texture(g, emu, t)?;
        if matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
            for p in px.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
        }
        // Metal drawables hold premultiplied colors, as tiny-skia does.
        let size = tiny_skia::IntSize::from_wh(w, h).ok_or("empty drawable")?;
        let pix = tiny_skia::Pixmap::from_vec(px, size).ok_or("bad drawable pixels")?;
        metal::show_frame(emu, layer, pix).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Write a texture as a PNG (debugging).
fn dump(g: &mut Gpu, emu: &Emu, t: u64, path: &str) -> R<()> {
    let format = texture(g, emu, t)?.format;
    let (px, w, h) = read_texture(g, emu, t)?;
    let bpp = (px.len() / (w * h) as usize).max(1);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for p in px.chunks_exact(bpp) {
        match (bpp, format) {
            (1, _) => rgba.extend_from_slice(&[p[0], p[0], p[0], 255]),
            (4, wgpu::TextureFormat::Bgra8Unorm) => rgba.extend_from_slice(&[p[2], p[1], p[0], p[3]]),
            (4, _) => rgba.extend_from_slice(p),
            _ => rgba.extend_from_slice(&[p[0], 0, 0, 255]),
        }
    }
    let size = tiny_skia::IntSize::from_wh(w, h).ok_or("empty")?;
    let pix = tiny_skia::Pixmap::from_vec(rgba, size).ok_or("bad pixels")?;
    pix.save_png(path).map_err(|e| e.to_string())
}

fn write(g: &mut Gpu, emu: &Emu, t: u64, origin: [u64; 2], size: [u64; 2], bytes_per_row: u64, data: &[u8]) -> R<()> {
    texture(g, emu, t)?;
    let tex = &g.textures[&t];
    g.queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex.texture, mip_level: 0, origin: wgpu::Origin3d { x: origin[0] as u32, y: origin[1] as u32, z: 0 }, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bytes_per_row as u32), rows_per_image: Some(size[1] as u32) },
        wgpu::Extent3d { width: size[0] as u32, height: size[1] as u32, depth_or_array_layers: 1 },
    );
    Ok(())
}

/// `replaceRegion:…`: a CPU write to a texture, applied now.
pub(crate) fn write_texture(emu: &mut Emu, t: u64, origin: [u64; 2], size: [u64; 2], bytes_per_row: u64, data: &[u8]) -> R<()> {
    let mut g = gpu(emu)?;
    let r = write(&mut g, emu, t, origin, size, bytes_per_row, data);
    emu.metal.gpu = Some(g);
    r
}
