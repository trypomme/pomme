//! Metal: devices, queues, command buffers and encoders, buffers, textures,
//! shader libraries and pipelines, and CAMetalLayer drawables.
//!
//! Objects behave like Metal's: buffers' contents live in guest memory,
//! descriptors are property bags (snapshotted when a pipeline, sampler or
//! depth-stencil state is made from them), and encoders record commands into
//! their command buffer. `commit` runs them (metal_gpu.rs, on wgpu, with the
//! shaders translated by pomme-msl) and presents drawables into their
//! CAMetalLayer's contents. Shader sources can be dumped with POMME_DUMP_MSL=dir.

use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result, foundation};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Obj {
    Device,
    Queue,
    CommandBuffer { committed: bool, present: Vec<u64>, handlers: Vec<u64> },
    RenderEncoder { buffer: u64, pass: u64 },
    BlitEncoder { buffer: u64 },
    Buffer { contents: u64, length: u64 },
    Texture(Texture),
    /// MSL source, specialized with these function constants (index → value).
    Library { source: String, constants: Vec<(u32, f64)> },
    /// A precompiled library (`newLibraryWithData:`): function name → substituted MSL.
    BinaryLibrary { functions: HashMap<String, String> },
    Function { name: String, library: u64 },
    Pipeline(Box<PipelineDesc>),
    Sampler(SamplerDesc),
    DepthStencil(DepthStencilDesc),
    Drawable { layer: u64, texture: u64 },
    /// An IOSurface: its size and, once a texture is made on it, that texture
    /// (whose pixels are the surface's: shown when a layer's contents is set to it).
    Surface { width: u64, height: u64, bytes_per_row: u64, alloc: u64, texture: u64 },
}

#[derive(Debug, Clone, Default)]
pub struct Texture {
    pub width: u64,
    pub height: u64,
    pub format: u64,
    pub usage: u64,
    pub samples: u64,
}

/// A render pipeline's descriptor, as it was when the pipeline was made.
#[derive(Debug, Clone, Default)]
pub struct PipelineDesc {
    /// (library, entry point name).
    pub vertex: Option<(u64, String)>,
    pub fragment: Option<(u64, String)>,
    /// (attribute index, MTLVertexFormat, offset, buffer index).
    pub attributes: Vec<(u32, u64, u64, u32)>,
    /// (buffer index, stride, MTLVertexStepFunction, step rate).
    pub layouts: Vec<(u32, u64, u64, u64)>,
    pub colors: Vec<ColorDesc>,
    pub depth_format: u64,
    pub stencil_format: u64,
    pub samples: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ColorDesc {
    pub index: u32,
    pub format: u64,
    pub blending: bool,
    /// MTLBlendFactor / MTLBlendOperation: (source RGB, destination RGB, RGB op, source A, destination A, A op).
    pub blend: [u64; 6],
    /// MTLColorWriteMask (R 8, G 4, B 2, A 1).
    pub write_mask: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SamplerDesc {
    /// MTLSamplerMinMagFilter / MTLSamplerMipFilter.
    pub min: u64,
    pub mag: u64,
    pub mip: u64,
    /// MTLSamplerAddressMode for s, t.
    pub address: [u64; 2],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StencilDesc {
    pub compare: u64,
    /// MTLStencilOperation: (stencil fail, depth fail, pass).
    pub ops: [u64; 3],
    pub read_mask: u32,
    pub write_mask: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DepthStencilDesc {
    pub depth_compare: u64,
    pub depth_write: bool,
    pub front: Option<StencilDesc>,
    pub back: Option<StencilDesc>,
}

/// Where a draw reads a buffer slot from.
#[derive(Debug, Clone)]
pub enum BufRef {
    /// A Metal buffer at an offset.
    Buffer(u64, u64),
    /// `set…Bytes:length:atIndex:` (copied when set).
    Bytes(Vec<u8>),
}

/// Encoder state each draw captures.
#[derive(Debug, Clone, Default)]
pub struct DrawState {
    pub pipeline: u64,
    pub depth_stencil: u64,
    /// Front, back.
    pub stencil_ref: [u32; 2],
    /// x, y, width, height.
    pub scissor: Option<[u64; 4]>,
    /// x, y, width, height, near, far.
    pub viewport: Option<[f64; 6]>,
    pub cull: u64,
    pub winding: u64,
    pub blend_color: [f64; 4],
    /// Per stage (vertex 0, fragment 1): buffer, texture and sampler slots.
    pub buffers: [std::collections::BTreeMap<u32, BufRef>; 2],
    pub textures: [std::collections::BTreeMap<u32, u64>; 2],
    pub samplers: [std::collections::BTreeMap<u32, u64>; 2],
}

#[derive(Debug, Clone)]
pub enum DrawCall {
    Arrays { start: u64, count: u64, instances: u64, base_instance: u64 },
    /// MTLIndexType 0 = u16, 1 = u32.
    Indexed { count: u64, index_type: u64, buffer: u64, offset: u64, instances: u64, base_vertex: i64, base_instance: u64 },
}

#[derive(Debug, Clone)]
pub struct Draw {
    pub state: DrawState,
    /// MTLPrimitiveType.
    pub primitive: u64,
    pub call: DrawCall,
}

#[derive(Debug, Clone, Default)]
pub struct Attachment {
    pub texture: u64,
    pub resolve: u64,
    /// MTLLoadAction / MTLStoreAction.
    pub load: u64,
    pub store: u64,
    pub clear_color: [f64; 4],
    pub clear_depth: f64,
    pub clear_stencil: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Pass {
    /// (color attachment index, attachment).
    pub colors: Vec<(u32, Attachment)>,
    pub depth: Option<Attachment>,
    pub stencil: Option<Attachment>,
    pub draws: Vec<Draw>,
}

#[derive(Debug, Clone)]
pub enum Cmd {
    Pass(Pass),
    CopyBuffer { src: u64, src_offset: u64, dst: u64, dst_offset: u64, size: u64 },
    /// Buffer → texture: (buffer, offset, bytes per row, texture, origin x, y, width, height).
    BufferToTexture { buffer: u64, offset: u64, bytes_per_row: u64, texture: u64, origin: [u64; 2], size: [u64; 2] },
    TextureToTexture { src: u64, src_origin: [u64; 2], size: [u64; 2], dst: u64, dst_origin: [u64; 2] },
}

#[derive(Default)]
pub struct State {
    pub objects: HashMap<u64, Obj>,
    /// CAMetalLayer → its settings.
    pub layers: HashMap<u64, MetalLayer>,
    /// Every shader source compiled, in order (also dumped with POMME_DUMP_MSL).
    pub sources: Vec<String>,
    /// Encoder calls by selector, for the bring-up report.
    pub calls: HashMap<String, u64>,
    /// The one MTLCaptureManager.
    pub capture_manager: u64,
    /// Drawables presented by committed command buffers.
    pub presented: u64,
    /// MTLFunctionConstantValues → the values set on it (index → value).
    constant_values: HashMap<u64, Vec<(u32, f64)>>,
    /// MTLRenderPassColorAttachmentDescriptor → its clear color (four doubles, d0–d3).
    clear_colors: HashMap<u64, [f64; 4]>,
    device: u64,
    /// Descriptor arrays → the class of their elements.
    element_class: HashMap<u64, &'static str>,
    /// Commands recorded into each command buffer.
    pub cmds: HashMap<u64, Vec<Cmd>>,
    /// Open render encoders: (command buffer, the pass so far, current state).
    encoders: HashMap<u64, (u64, Pass, DrawState)>,
    /// Open blit encoders → their command buffer.
    blits: HashMap<u64, u64>,
    /// The GPU, once something runs on it.
    #[cfg(feature = "gpu")]
    pub gpu: Option<crate::metal_gpu::Gpu>,
    /// Why the GPU couldn't start, if it couldn't (reported once).
    pub gpu_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MetalLayer {
    pub device: u64,
    pub pixel_format: u64,
    pub drawable_size: [f64; 2],
    pub framebuffer_only: bool,
}

/// An IEEE half's value.
pub fn half_to_f64(h: u16) -> f64 {
    let (s, e, m) = ((h >> 15) & 1, ((h >> 10) & 31) as i32, (h & 1023) as f64);
    let v = match e {
        0 => m * 2f64.powi(-24),
        31 => if m == 0.0 { f64::INFINITY } else { f64::NAN },
        _ => (1.0 + m / 1024.0) * 2f64.powi(e - 15),
    };
    if s == 1 { -v } else { v }
}

/// MTLPixelFormatBGRA8Unorm.
pub const BGRA8: u64 = 80;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn new_obj(emu: &mut Emu, class: &str, obj: Obj) -> Result<u64> {
    let cls = host_class(emu, class)?;
    let o = objc::create_instance(emu, cls, 0)?;
    emu.metal.objects.insert(o, obj);
    Ok(o)
}

fn obj(emu: &Emu, o: u64) -> Option<&Obj> {
    emu.metal.objects.get(&o)
}

fn count(emu: &mut Emu, sel: &str) {
    *emu.metal.calls.entry(sel.to_string()).or_default() += 1;
}

fn trace(emu: &Emu, what: impl FnOnce() -> String) {
    if emu.trace || std::env::var("POMME_TRACE_METAL").is_ok() {
        println!("[metal] {}", what());
    }
}

// ---- device ----

fn system_default_device(emu: &mut Emu) -> Result<()> {
    if emu.metal.device == 0 {
        let dev = new_obj(emu, "MTLPommeDevice", Obj::Device)?;
        emu.metal.device = dev;
    }
    let dev = emu.metal.device;
    send(emu, dev, "retain", &[])?;
    ret(emu, dev)
}

fn device_name(emu: &mut Emu) -> Result<()> {
    let s = foundation::new_nsstring(emu, "pomme GPU (wgpu)")?;
    ret(emu, s)
}

fn new_queue(emu: &mut Emu) -> Result<()> {
    let q = new_obj(emu, "MTLPommeCommandQueue", Obj::Queue)?;
    ret(emu, q)
}

/// `newBufferWithLength:options:` / `newBufferWithBytes:length:options:`.
fn new_buffer(emu: &mut Emu, bytes: Option<u64>, length: u64) -> Result<()> {
    let contents = emu.alloc(length.max(16))?;
    if let Some(src) = bytes.filter(|&p| p != 0) {
        let data = emu.p.memory.read(src, length as usize)?.to_vec();
        emu.p.memory.write(contents, &data)?;
    }
    let b = new_obj(emu, "MTLPommeBuffer", Obj::Buffer { contents, length })?;
    trace(emu, || format!("buffer {b:#x}: {length} bytes"));
    ret(emu, b)
}

fn new_texture(emu: &mut Emu) -> Result<()> {
    let desc = emu.arg(2);
    let get = |emu: &Emu, k: &str| objc::bag_get(emu, desc, k).unwrap_or(0);
    let t = Texture { width: get(emu, "width"), height: get(emu, "height"), format: get(emu, "pixelFormat"), usage: get(emu, "usage"), samples: get(emu, "sampleCount").max(1) };
    trace(emu, || format!("texture {}×{} format {} ×{} usage {}", t.width, t.height, t.format, t.samples, t.usage));
    let o = new_obj(emu, "MTLPommeTexture", Obj::Texture(t))?;
    ret(emu, o)
}

/// `newTextureWithDescriptor:iosurface:plane:`: a texture whose storage is the surface.
fn new_texture_on_surface(emu: &mut Emu) -> Result<()> {
    let surface = emu.arg(3);
    new_texture(emu)?;
    let t = emu.cpu.x[0];
    if let Some(Obj::Surface { texture, .. }) = emu.metal.objects.get_mut(&surface) {
        *texture = t;
    }
    ret(emu, t)
}

// ---- IOSurface (Flutter's FlutterMetalLayer renders into them) ----

/// A number from an IOSurface properties dictionary, by key name.
fn dict_number(emu: &mut Emu, dict: u64, key: &str) -> Result<u64> {
    let k = foundation::new_nsstring(emu, key)?;
    let v = send(emu, dict, "objectForKey:", &[k])?;
    if v == 0 {
        return Ok(0);
    }
    send(emu, v, "unsignedLongLongValue", &[])
}

fn iosurface_create(emu: &mut Emu) -> Result<()> {
    let dict = emu.arg(0);
    let width = dict_number(emu, dict, "kIOSurfaceWidth")?;
    let height = dict_number(emu, dict, "kIOSurfaceHeight")?;
    let bpe = dict_number(emu, dict, "kIOSurfaceBytesPerElement")?.max(1);
    let bytes_per_row = dict_number(emu, dict, "kIOSurfaceBytesPerRow")?.max(width * bpe);
    let alloc = dict_number(emu, dict, "kIOSurfaceAllocSize")?.max(bytes_per_row * height);
    if width == 0 || height == 0 {
        return ret(emu, 0);
    }
    trace(emu, || format!("IOSurface {width}×{height}, {bytes_per_row} bytes per row"));
    let o = new_obj(emu, "IOSurface", Obj::Surface { width, height, bytes_per_row, alloc, texture: 0 })?;
    ret(emu, o)
}

fn surface_field(emu: &mut Emu, f: fn(&Obj) -> u64) -> Result<()> {
    let v = obj(emu, emu.arg(0)).map_or(0, f);
    ret(emu, v)
}

fn surface_width(o: &Obj) -> u64 {
    if let Obj::Surface { width, .. } = o { *width } else { 0 }
}
fn surface_height(o: &Obj) -> u64 {
    if let Obj::Surface { height, .. } = o { *height } else { 0 }
}
fn surface_bpr(o: &Obj) -> u64 {
    if let Obj::Surface { bytes_per_row, .. } = o { *bytes_per_row } else { 0 }
}
fn surface_alloc(o: &Obj) -> u64 {
    if let Obj::Surface { alloc, .. } = o { *alloc } else { 0 }
}

/// `IOSurfaceAlignProperty(key, value)`: rows and sizes rounded up as the GPU wants.
fn iosurface_align(emu: &mut Emu) -> Result<()> {
    let (key, v) = (emu.arg(0), emu.arg(1));
    let k = foundation::nsstring(emu, key).unwrap_or_default();
    let a = if k.contains("AllocSize") { 16384 } else if k.contains("BytesPerRow") { 64 } else { 1 };
    ret(emu, v.div_ceil(a) * a)
}

/// A layer's contents was set to `contents`: an IOSurface shows its texture's pixels.
pub(crate) fn contents_set(emu: &mut Emu, layer: u64, contents: u64) -> Result<()> {
    let Some(Obj::Surface { texture, .. }) = obj(emu, contents).cloned() else { return Ok(()) };
    if texture == 0 {
        return Ok(());
    }
    #[cfg(feature = "gpu")]
    match crate::metal_gpu::texture_pixmap(emu, texture) {
        Ok(pix) => show_frame(emu, layer, pix)?,
        Err(e) => println!("[metal] can't show IOSurface: {e}"),
    }
    #[cfg(not(feature = "gpu"))]
    let _ = layer;
    Ok(())
}

fn new_library_with_source(emu: &mut Emu) -> Result<()> {
    let src = emu.arg(2);
    let source = foundation::nsstring(emu, src)?;
    let n = emu.metal.sources.len();
    if let Ok(dir) = std::env::var("POMME_DUMP_MSL") {
        let _ = std::fs::write(format!("{dir}/shader-{n:03}.metal"), &source);
    }
    trace(emu, || format!("library #{n}: {} bytes of MSL", source.len()));
    emu.metal.sources.push(source.clone());
    let lib = new_obj(emu, "MTLPommeLibrary", Obj::Library { source, constants: Vec::new() })?;
    ret(emu, lib)
}

/// `newLibraryWithSource:options:completionHandler:`: compile, then call back (library, nil).
fn new_library_async(emu: &mut Emu) -> Result<()> {
    let block = emu.arg(4);
    new_library_with_source(emu)?;
    let lib = emu.cpu.x[0];
    crate::libsystem::call_block(emu, block, &[lib, 0])?;
    Ok(())
}

/// The function names in a `.metallib` (its function list's NAME tags).
pub fn metallib_functions(m: &[u8]) -> Option<Vec<String>> {
    if !m.starts_with(b"MTLB") {
        return None;
    }
    let u64_at = |o: usize| m.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()) as usize);
    let u32_at = |o: usize| m.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let list = u64_at(0x18)?;
    let count = u32_at(list)?;
    let mut p = list + 4;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let len = u32_at(p)?;
        let entry = m.get(p + 4..p + len)?;
        let mut q = 0;
        while q + 6 <= entry.len() && &entry[q..q + 4] != b"ENDT" {
            let tl = u16::from_le_bytes(entry[q + 4..q + 6].try_into().unwrap()) as usize;
            if &entry[q..q + 4] == b"NAME" {
                let raw = entry.get(q + 6..q + 6 + tl)?;
                names.push(String::from_utf8_lossy(raw).trim_end_matches('\0').to_string());
            }
            q += 6 + tl;
        }
        p += len;
    }
    Some(names)
}

/// Where substituted MSL for a precompiled library's functions lives: a Flutter
/// engine's built-in Impeller shaders, generated by scripts/flutter/impeller-msl.sh
/// for the revision in the app's Flutter.framework.
fn substitute_msl(emu: &mut Emu, name: &str) -> Option<String> {
    let sysroot = emu.p.sysroot.clone()?;
    let fw = emu.p.images.iter().find(|i| i.path.to_string_lossy().contains("Flutter.framework/"))?.path.parent()?.to_path_buf();
    let info = pomme_loader::vfs::read(&fw.join("Info.plist")).ok()?;
    let rev = plist::Value::from_reader(std::io::Cursor::new(info)).ok()?.as_dictionary()?.get("FlutterEngine")?.as_string()?.to_string();
    let file = sysroot.join("pomme/impeller").join(rev).join(format!("{name}.metal"));
    pomme_loader::vfs::read(&file).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// `newLibraryWithData:error:` (a dispatch_data metallib): its functions get substituted MSL.
fn new_library_with_data(emu: &mut Emu) -> Result<()> {
    let (data, error_out) = (emu.arg(2), emu.arg(3));
    let bytes = crate::libsystem::dispatch_data_bytes(emu, data)?;
    let names = metallib_functions(&bytes).unwrap_or_default();
    let mut functions = HashMap::new();
    let mut missing = Vec::new();
    for n in &names {
        match substitute_msl(emu, n) {
            Some(src) => {
                functions.insert(n.clone(), src);
            }
            None => missing.push(n.clone()),
        }
    }
    trace(emu, || format!("binary library: {} functions, {} with MSL", names.len(), functions.len()));
    if !missing.is_empty() && functions.is_empty() {
        println!("[metal] a precompiled shader library ({} functions) has no substitute MSL (Flutter: run scripts/flutter/impeller-msl.sh)", names.len());
    }
    let _ = error_out;
    let lib = new_obj(emu, "MTLPommeLibrary", Obj::BinaryLibrary { functions })?;
    ret(emu, lib)
}

fn new_function(emu: &mut Emu) -> Result<()> {
    let (lib, name) = (emu.arg(0), emu.arg(2));
    let f = make_function(emu, lib, name, 0)?;
    ret(emu, f)
}

/// A function from a library, specialized with an MTLFunctionConstantValues (or 0).
fn make_function(emu: &mut Emu, lib: u64, name: u64, constants: u64) -> Result<u64> {
    let name = foundation::nsstring(emu, name)?;
    let constants = emu.metal.constant_values.get(&constants).cloned().unwrap_or_default();
    let source = match obj(emu, lib) {
        Some(Obj::BinaryLibrary { functions }) => match functions.get(&name) {
            Some(src) => src.clone(),
            None => return Ok(0),
        },
        Some(Obj::Library { source, .. }) => source.clone(),
        _ => return Ok(0),
    };
    if let Ok(dir) = std::env::var("POMME_DUMP_MSL") {
        let _ = std::fs::write(format!("{dir}/{name}.metal"), &source);
    }
    emu.metal.sources.push(source.clone());
    // Each function gets a library of its own (specialized when constants are given).
    let single = new_obj(emu, "MTLPommeLibrary", Obj::Library { source, constants })?;
    new_obj(emu, "MTLPommeFunction", Obj::Function { name, library: single })
}

/// A property of a descriptor-bag object, or Metal's default when never set.
fn prop(emu: &Emu, o: u64, key: &str, default: u64) -> u64 {
    if o == 0 { default } else { objc::bag_get(emu, o, key).unwrap_or(default) }
}

/// Element `i` of a descriptor array property (`colorAttachments`, …), if it was ever touched.
fn element(emu: &Emu, o: u64, key: &str, i: u32) -> u64 {
    let arr = prop(emu, o, key, 0);
    prop(emu, arr, &i.to_string(), 0)
}

fn function_ref(emu: &Emu, f: u64) -> Option<(u64, String)> {
    match obj(emu, f) {
        Some(Obj::Function { name, library }) => Some((*library, name.clone())),
        _ => None,
    }
}

fn snapshot_pipeline(emu: &Emu, d: u64) -> PipelineDesc {
    let vd = prop(emu, d, "vertexDescriptor", 0);
    let mut p = PipelineDesc {
        vertex: function_ref(emu, prop(emu, d, "vertexFunction", 0)),
        fragment: function_ref(emu, prop(emu, d, "fragmentFunction", 0)),
        depth_format: prop(emu, d, "depthAttachmentPixelFormat", 0),
        stencil_format: prop(emu, d, "stencilAttachmentPixelFormat", 0),
        samples: prop(emu, d, "rasterSampleCount", prop(emu, d, "sampleCount", 1)).max(1),
        ..Default::default()
    };
    for i in 0..31 {
        let a = element(emu, vd, "attributes", i);
        if a != 0 && prop(emu, a, "format", 0) != 0 {
            p.attributes.push((i, prop(emu, a, "format", 0), prop(emu, a, "offset", 0), prop(emu, a, "bufferIndex", 0) as u32));
        }
        let l = element(emu, vd, "layouts", i);
        if l != 0 && prop(emu, l, "stride", 0) != 0 {
            p.layouts.push((i, prop(emu, l, "stride", 0), prop(emu, l, "stepFunction", 1), prop(emu, l, "stepRate", 1)));
        }
    }
    for i in 0..8 {
        let c = element(emu, d, "colorAttachments", i);
        let format = prop(emu, c, "pixelFormat", 0);
        if c == 0 || format == 0 {
            continue;
        }
        p.colors.push(ColorDesc {
            index: i,
            format,
            blending: prop(emu, c, "blendingEnabled", 0) & 0xff != 0,
            blend: [
                prop(emu, c, "sourceRGBBlendFactor", 1),
                prop(emu, c, "destinationRGBBlendFactor", 0),
                prop(emu, c, "rgbBlendOperation", 0),
                prop(emu, c, "sourceAlphaBlendFactor", 1),
                prop(emu, c, "destinationAlphaBlendFactor", 0),
                prop(emu, c, "alphaBlendOperation", 0),
            ],
            write_mask: prop(emu, c, "writeMask", 15),
        });
    }
    p
}

fn new_pipeline(emu: &mut Emu) -> Result<()> {
    let desc = emu.arg(2);
    let d = snapshot_pipeline(emu, desc);
    let label = format!("{} / {}", d.vertex.as_ref().map_or("-", |v| v.1.as_str()), d.fragment.as_ref().map_or("-", |f| f.1.as_str()));
    let p = new_obj(emu, "MTLPommeRenderPipelineState", Obj::Pipeline(Box::new(d)))?;
    trace(emu, || format!("pipeline {p:#x}: {label}"));
    ret(emu, p)
}

fn new_sampler(emu: &mut Emu) -> Result<()> {
    let d = emu.arg(2);
    let desc = SamplerDesc {
        min: prop(emu, d, "minFilter", 0),
        mag: prop(emu, d, "magFilter", 0),
        mip: prop(emu, d, "mipFilter", 0),
        address: [prop(emu, d, "sAddressMode", 0), prop(emu, d, "tAddressMode", 0)],
    };
    let s = new_obj(emu, "MTLPommeSamplerState", Obj::Sampler(desc))?;
    ret(emu, s)
}

fn stencil_desc(emu: &Emu, s: u64) -> Option<StencilDesc> {
    (s != 0).then(|| StencilDesc {
        compare: prop(emu, s, "stencilCompareFunction", 7),
        ops: [prop(emu, s, "stencilFailureOperation", 0), prop(emu, s, "depthFailureOperation", 0), prop(emu, s, "depthStencilPassOperation", 0)],
        read_mask: prop(emu, s, "readMask", 0xffff_ffff) as u32,
        write_mask: prop(emu, s, "writeMask", 0xffff_ffff) as u32,
    })
}

fn new_depth_stencil(emu: &mut Emu) -> Result<()> {
    let d = emu.arg(2);
    let desc = DepthStencilDesc {
        depth_compare: prop(emu, d, "depthCompareFunction", 7),
        depth_write: prop(emu, d, "depthWriteEnabled", 0) & 0xff != 0,
        front: stencil_desc(emu, prop(emu, d, "frontFaceStencil", 0)),
        back: stencil_desc(emu, prop(emu, d, "backFaceStencil", 0)),
    };
    let s = new_obj(emu, "MTLPommeDepthStencilState", Obj::DepthStencil(desc))?;
    ret(emu, s)
}

/// `newRenderPipelineStateWithDescriptor:completionHandler:` → handler(state, nil).
fn new_pipeline_async(emu: &mut Emu) -> Result<()> {
    let block = emu.arg(3);
    new_pipeline(emu)?;
    let p = emu.cpu.x[0];
    crate::libsystem::call_block(emu, block, &[p, 0])?;
    Ok(())
}

// ---- command buffers and encoders ----

fn new_command_buffer(emu: &mut Emu) -> Result<()> {
    let b = new_obj(emu, "MTLPommeCommandBuffer", Obj::CommandBuffer { committed: false, present: Vec::new(), handlers: Vec::new() })?;
    // Command buffers from `commandBuffer` are autoreleased.
    send(emu, b, "autorelease", &[])?;
    ret(emu, b)
}

fn attachment(emu: &Emu, a: u64) -> Option<Attachment> {
    let texture = prop(emu, a, "texture", 0);
    (texture != 0).then(|| Attachment {
        texture,
        resolve: prop(emu, a, "resolveTexture", 0),
        load: prop(emu, a, "loadAction", 0),
        store: prop(emu, a, "storeAction", 0),
        clear_color: emu.metal.clear_colors.get(&a).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]),
        clear_depth: objc::bag_get_f64(emu, a, "clearDepth").unwrap_or(1.0),
        clear_stencil: prop(emu, a, "clearStencil", 0) as u32,
    })
}

fn render_encoder(emu: &mut Emu) -> Result<()> {
    let (cb, desc) = (emu.arg(0), emu.arg(2));
    let mut pass = Pass::default();
    for i in 0..8 {
        if let Some(a) = attachment(emu, element(emu, desc, "colorAttachments", i)) {
            pass.colors.push((i, a));
        }
    }
    pass.depth = attachment(emu, prop(emu, desc, "depthAttachment", 0));
    pass.stencil = attachment(emu, prop(emu, desc, "stencilAttachment", 0));
    let e = new_obj(emu, "MTLPommeRenderCommandEncoder", Obj::RenderEncoder { buffer: cb, pass: desc })?;
    emu.metal.encoders.insert(e, (cb, pass, DrawState { cull: 0, winding: 0, ..Default::default() }));
    count(emu, "renderCommandEncoderWithDescriptor:");
    send(emu, e, "autorelease", &[])?;
    ret(emu, e)
}

fn blit_encoder(emu: &mut Emu) -> Result<()> {
    let cb = emu.arg(0);
    let e = new_obj(emu, "MTLPommeBlitCommandEncoder", Obj::BlitEncoder { buffer: cb })?;
    emu.metal.blits.insert(e, cb);
    count(emu, "blitCommandEncoder");
    send(emu, e, "autorelease", &[])?;
    ret(emu, e)
}

fn present_drawable(emu: &mut Emu) -> Result<()> {
    let (cb, drawable) = (emu.arg(0), emu.arg(2));
    send(emu, drawable, "retain", &[])?;
    if let Some(Obj::CommandBuffer { present, .. }) = emu.metal.objects.get_mut(&cb) {
        present.push(drawable);
    }
    Ok(())
}

fn add_completed_handler(emu: &mut Emu) -> Result<()> {
    let (cb, block) = (emu.arg(0), emu.arg(2));
    let saved = emu.cpu.x[0];
    emu.cpu.x[0] = block;
    crate::libc::block_copy(emu)?;
    let copy = emu.cpu.x[0];
    emu.cpu.x[0] = saved;
    if let Some(Obj::CommandBuffer { handlers, .. }) = emu.metal.objects.get_mut(&cb) {
        handlers.push(copy);
    }
    Ok(())
}

/// `commit`: run the buffer (recorded only, for now), then its completion handlers.
fn commit(emu: &mut Emu) -> Result<()> {
    let cb = emu.arg(0);
    let Some(Obj::CommandBuffer { handlers, present, .. }) = obj(emu, cb).cloned() else { return Ok(()) };
    if let Some(Obj::CommandBuffer { committed, .. }) = emu.metal.objects.get_mut(&cb) {
        *committed = true;
    }
    trace(emu, || format!("commit {cb:#x}: {} drawables, {} handlers", present.len(), handlers.len()));
    let cmds = emu.metal.cmds.remove(&cb).unwrap_or_default();
    execute(emu, cmds, &present)?;
    emu.metal.presented += present.len() as u64;
    // An app's own CAMetalDrawable (Flutter's FlutterDrawable) presents itself.
    for &d in &present {
        if !matches!(obj(emu, d), Some(Obj::Drawable { .. })) {
            send(emu, d, "present", &[])?;
        }
    }
    for h in handlers {
        crate::libsystem::call_block(emu, h, &[cb])?;
    }
    Ok(())
}

// ---- recording ----

fn enc_state(emu: &mut Emu) -> Option<&mut DrawState> {
    let e = emu.arg(0);
    emu.metal.encoders.get_mut(&e).map(|(_, _, st)| st)
}

/// The encoder's stage index from a selector prefix (vertex 0, fragment 1).
fn set_buffer(emu: &mut Emu, stage: usize) -> Result<()> {
    let (b, off, i) = (emu.arg(2), emu.arg(3), emu.arg(4) as u32);
    if let Some(st) = enc_state(emu) {
        st.buffers[stage].insert(i, BufRef::Buffer(b, off));
    }
    Ok(())
}

fn set_buffer_offset(emu: &mut Emu, stage: usize) -> Result<()> {
    let (off, i) = (emu.arg(2), emu.arg(3) as u32);
    if let Some(st) = enc_state(emu)
        && let Some(BufRef::Buffer(_, o)) = st.buffers[stage].get_mut(&i)
    {
        *o = off;
    }
    Ok(())
}

fn set_bytes(emu: &mut Emu, stage: usize) -> Result<()> {
    let (p, n, i) = (emu.arg(2), emu.arg(3), emu.arg(4) as u32);
    let bytes = emu.p.memory.read(p, n as usize)?.to_vec();
    if let Some(st) = enc_state(emu) {
        st.buffers[stage].insert(i, BufRef::Bytes(bytes));
    }
    Ok(())
}

fn set_texture(emu: &mut Emu, stage: usize) -> Result<()> {
    let (t, i) = (emu.arg(2), emu.arg(3) as u32);
    if let Some(st) = enc_state(emu) {
        st.textures[stage].insert(i, t);
    }
    Ok(())
}

fn set_sampler(emu: &mut Emu, stage: usize) -> Result<()> {
    let (t, i) = (emu.arg(2), emu.arg(3) as u32);
    if let Some(st) = enc_state(emu) {
        st.samplers[stage].insert(i, t);
    }
    Ok(())
}

fn push_draw(emu: &mut Emu, primitive: u64, call: DrawCall) -> Result<()> {
    let e = emu.arg(0);
    count(emu, "draw");
    if let Some((_, pass, st)) = emu.metal.encoders.get_mut(&e) {
        pass.draws.push(Draw { state: st.clone(), primitive, call });
    }
    Ok(())
}

fn draw_arrays(emu: &mut Emu, with_instances: bool, with_base: bool) -> Result<()> {
    let (prim, start, n) = (emu.arg(2), emu.arg(3), emu.arg(4));
    let instances = if with_instances { emu.arg(5) } else { 1 };
    let base_instance = if with_base { emu.arg(6) } else { 0 };
    push_draw(emu, prim, DrawCall::Arrays { start, count: n, instances, base_instance })
}

fn draw_indexed(emu: &mut Emu, with_instances: bool, with_bases: bool) -> Result<()> {
    let (prim, n, index_type, buffer, offset) = (emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5), emu.arg(6));
    let instances = if with_instances { emu.arg(7) } else { 1 };
    // The ninth and tenth arguments (base vertex, base instance) are on the stack.
    let (base_vertex, base_instance) = if with_bases { (emu.read_u64(emu.cpu.sp)? as i64, emu.read_u64(emu.cpu.sp + 8)?) } else { (0, 0) };
    push_draw(emu, prim, DrawCall::Indexed { count: n, index_type, buffer, offset, instances, base_vertex, base_instance })
}

fn end_render_encoding(emu: &mut Emu) -> Result<()> {
    let e = emu.arg(0);
    if let Some((cb, pass, _)) = emu.metal.encoders.remove(&e) {
        trace(emu, || format!("pass: {} draws", pass.draws.len()));
        emu.metal.cmds.entry(cb).or_default().push(Cmd::Pass(pass));
    }
    Ok(())
}

fn blit_cmd(emu: &mut Emu, cmd: Cmd) {
    let e = emu.arg(0);
    if let Some(&cb) = emu.metal.blits.get(&e) {
        emu.metal.cmds.entry(cb).or_default().push(cmd);
    }
}

/// Three NSUIntegers (MTLOrigin / MTLSize), passed by reference.
fn triple(emu: &Emu, p: u64) -> Result<[u64; 3]> {
    Ok([emu.read_u64(p)?, emu.read_u64(p + 8)?, emu.read_u64(p + 16)?])
}

/// Run a committed command buffer's commands, then present its drawables.
fn execute(emu: &mut Emu, cmds: Vec<Cmd>, present: &[u64]) -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        if let Err(e) = crate::metal_gpu::execute(emu, &cmds, present)
            && emu.metal.gpu_error.as_deref() != Some(e.as_str())
        {
            println!("[metal] {e}");
            emu.metal.gpu_error = Some(e);
        }
    }
    #[cfg(not(feature = "gpu"))]
    {
        let _ = present;
        // Without a GPU, still do what the CPU can see: buffer copies.
        for c in &cmds {
            if let Cmd::CopyBuffer { src, src_offset, dst, dst_offset, size } = c {
                copy_buffer(emu, *src, *src_offset, *dst, *dst_offset, *size)?;
            }
        }
    }
    Ok(())
}

/// A buffer → buffer copy, in guest memory (where buffers live).
pub(crate) fn copy_buffer(emu: &mut Emu, src: u64, src_offset: u64, dst: u64, dst_offset: u64, size: u64) -> Result<()> {
    let (Some(Obj::Buffer { contents: s, .. }), Some(Obj::Buffer { contents: d, .. })) = (obj(emu, src).cloned(), obj(emu, dst).cloned()) else { return Ok(()) };
    let bytes = emu.p.memory.read(s + src_offset, size as usize)?.to_vec();
    emu.p.memory.write(d + dst_offset, &bytes)?;
    Ok(())
}

/// A buffer's bytes (in guest memory) from `offset`, at most `len`.
pub fn buffer_bytes(emu: &Emu, b: u64, offset: u64, len: u64) -> Option<Vec<u8>> {
    let Some(Obj::Buffer { contents, length }) = obj(emu, b) else { return None };
    let n = len.min(length.saturating_sub(offset));
    emu.p.memory.read(contents + offset, n as usize).ok().map(<[u8]>::to_vec)
}


// ---- CAMetalLayer ----

fn metal_layer(emu: &mut Emu, l: u64) -> &mut MetalLayer {
    emu.metal.layers.entry(l).or_insert_with(|| MetalLayer { pixel_format: BGRA8, framebuffer_only: true, ..Default::default() })
}

fn next_drawable(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let ml = metal_layer(emu, l).clone();
    let t = Texture { width: ml.drawable_size[0] as u64, height: ml.drawable_size[1] as u64, format: ml.pixel_format, usage: 4, samples: 1 };
    trace(emu, || format!("nextDrawable {}×{}", t.width, t.height));
    let tex = new_obj(emu, "MTLPommeTexture", Obj::Texture(t))?;
    let dr = new_obj(emu, "CAMetalDrawable", Obj::Drawable { layer: l, texture: tex })?;
    send(emu, dr, "autorelease", &[])?;
    ret(emu, dr)
}

fn drawable_texture(emu: &mut Emu) -> Result<()> {
    let t = match obj(emu, emu.arg(0)) {
        Some(Obj::Drawable { texture, .. }) => *texture,
        _ => 0,
    };
    ret(emu, t)
}

fn texture_dims(emu: &mut Emu, which: usize) -> Result<()> {
    let v = match obj(emu, emu.arg(0)) {
        // MTLTextureType: 2D (2), or 2D multisample (4).
        Some(Obj::Texture(t)) => [t.width, t.height, t.format, t.usage, t.samples, if t.samples > 1 { 4 } else { 2 }][which],
        _ => 0,
    };
    ret(emu, v)
}

fn buffer_contents(emu: &mut Emu) -> Result<()> {
    let p = match obj(emu, emu.arg(0)) {
        Some(Obj::Buffer { contents, .. }) => *contents,
        _ => 0,
    };
    ret(emu, p)
}

fn buffer_length(emu: &mut Emu) -> Result<()> {
    let n = match obj(emu, emu.arg(0)) {
        Some(Obj::Buffer { length, .. }) => *length,
        _ => 0,
    };
    ret(emu, n)
}

// ---- descriptors: property bags with lazily made sub-objects ----

/// `obj.key`, made on first use as an instance of `class` (attachment and
/// vertex descriptors hang off their parents this way).
fn child(emu: &mut Emu, o: u64, key: &str, class: &str) -> Result<u64> {
    if let Some(c) = objc::bag_get(emu, o, key).filter(|&c| c != 0) {
        return Ok(c);
    }
    let cls = host_class(emu, class)?;
    let c = objc::create_instance(emu, cls, 0)?;
    objc::bag_set(emu, o, key, c);
    Ok(c)
}

/// An indexed array of descriptors (`colorAttachments`, `attributes`, `layouts`).
fn array_of(emu: &mut Emu, o: u64, key: &str, element: &'static str) -> Result<()> {
    let a = child(emu, o, key, "MTLPommeDescriptorArray")?;
    emu.metal.element_class.insert(a, element);
    ret(emu, a)
}

fn array_at(emu: &mut Emu) -> Result<()> {
    let (a, i) = (emu.arg(0), emu.arg(2));
    let class = emu.metal.element_class.get(&a).copied().unwrap_or("NSObject");
    let e = child(emu, a, &format!("{i}"), class)?;
    ret(emu, e)
}

fn array_set_at(emu: &mut Emu) -> Result<()> {
    let (a, o, i) = (emu.arg(0), emu.arg(2), emu.arg(3));
    objc::bag_set(emu, a, &format!("{i}"), o);
    Ok(())
}

/// `replaceRegion:mipmapLevel:withBytes:bytesPerRow:`: the CPU writes texels now.
fn replace_region(emu: &mut Emu) -> Result<()> {
    let (t, region, bytes, bpr) = (emu.arg(0), emu.arg(2), emu.arg(4), emu.arg(5));
    let o = triple(emu, region)?;
    let sz = triple(emu, region + 24)?;
    let data = emu.p.memory.read(bytes, (bpr * sz[1]) as usize)?.to_vec();
    #[cfg(feature = "gpu")]
    if let Err(e) = crate::metal_gpu::write_texture(emu, t, [o[0], o[1]], [sz[0], sz[1]], bpr, &data) {
        println!("[metal] replaceRegion: {e}");
    }
    #[cfg(not(feature = "gpu"))]
    let _ = (t, o, data);
    Ok(())
}

/// Show a presented drawable: its pixels become the CAMetalLayer's contents.
pub(crate) fn show_frame(emu: &mut Emu, layer: u64, pix: tiny_skia::Pixmap) -> Result<()> {
    let scale = emu.device.scale;
    let size = [pix.width() as f64 / scale, pix.height() as f64 / scale];
    let img = crate::image::new_image(emu, crate::render::Image { pix: std::rc::Rc::new(pix), size, scale, caps: [0.0; 4], orientation: 0, look: Default::default() })?;
    crate::calayer::layer_mut(emu, layer).contents = img;
    crate::host::frame_changed(emu);
    Ok(())
}

/// The bring-up report: what the app compiled and encoded.
pub fn report(emu: &Emu) -> String {
    let mut calls: Vec<_> = emu.metal.calls.iter().collect();
    calls.sort_by(|a, b| b.1.cmp(a.1));
    let m = &emu.metal;
    let mut s = format!("metal: {} shader libraries, {} objects, {} frames presented\n", m.sources.len(), m.objects.len(), m.presented);
    for (sel, n) in calls {
        s += &format!("  {n:>6}  {sel}\n");
    }
    s
}

const DEV: &str = "MTLPommeDevice";
const RE: &str = "MTLPommeRenderCommandEncoder";
const BE: &str = "MTLPommeBlitCommandEncoder";
const ML: &str = "CAMetalLayer";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("MTLPommeDescriptorArray", false, "objectAtIndexedSubscript:", array_at),
    ("MTLPommeDescriptorArray", false, "setObject:atIndexedSubscript:", array_set_at),
    ("MTLRenderPassDescriptor", false, "colorAttachments", |e| {
        let o = e.arg(0);
        array_of(e, o, "colorAttachments", "MTLRenderPassColorAttachmentDescriptor")
    }),
    ("MTLRenderPassDescriptor", false, "depthAttachment", |e| {
        let o = e.arg(0);
        let c = child(e, o, "depthAttachment", "MTLRenderPassDepthAttachmentDescriptor")?;
        ret(e, c)
    }),
    ("MTLRenderPassDescriptor", false, "stencilAttachment", |e| {
        let o = e.arg(0);
        let c = child(e, o, "stencilAttachment", "MTLRenderPassStencilAttachmentDescriptor")?;
        ret(e, c)
    }),
    ("MTLRenderPipelineDescriptor", false, "colorAttachments", |e| {
        let o = e.arg(0);
        array_of(e, o, "colorAttachments", "MTLRenderPipelineColorAttachmentDescriptor")
    }),
    ("MTLRenderPipelineDescriptor", false, "vertexDescriptor", |e| {
        let o = e.arg(0);
        let c = child(e, o, "vertexDescriptor", "MTLVertexDescriptor")?;
        ret(e, c)
    }),
    ("MTLVertexDescriptor", false, "attributes", |e| {
        let o = e.arg(0);
        array_of(e, o, "attributes", "MTLVertexAttributeDescriptor")
    }),
    ("MTLVertexDescriptor", false, "layouts", |e| {
        let o = e.arg(0);
        array_of(e, o, "layouts", "MTLVertexBufferLayoutDescriptor")
    }),
    (DEV, false, "name", device_name),
    (DEV, false, "newCommandQueue", new_queue),
    (DEV, false, "newCommandQueueWithMaxCommandBufferCount:", new_queue),
    (DEV, false, "newBufferWithLength:options:", |e| {
        let n = e.arg(2);
        new_buffer(e, None, n)
    }),
    (DEV, false, "newBufferWithBytes:length:options:", |e| {
        let (p, n) = (e.arg(2), e.arg(3));
        new_buffer(e, Some(p), n)
    }),
    (DEV, false, "newTextureWithDescriptor:", new_texture),
    (DEV, false, "newTextureWithDescriptor:iosurface:plane:", new_texture_on_surface),
    ("MTLCaptureManager", true, "sharedCaptureManager", |e| {
        if e.metal.capture_manager == 0 {
            let cls = host_class(e, "MTLCaptureManager")?;
            e.metal.capture_manager = objc::create_instance(e, cls, 0)?;
        }
        let m = e.metal.capture_manager;
        ret(e, m)
    }),
    ("MTLTextureDescriptor", true, "texture2DDescriptorWithPixelFormat:width:height:mipmapped:", |e| {
        let (cls, format, w, h, mip) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4), e.arg(5) & 0xff != 0);
        let d = send(e, cls, "alloc", &[])?;
        let d = send(e, d, "init", &[])?;
        let levels = if mip { 64 - w.max(h).max(1).leading_zeros() as u64 } else { 1 };
        for (k, v) in [("pixelFormat", format), ("width", w), ("height", h), ("depth", 1), ("mipmapLevelCount", levels), ("sampleCount", 1), ("textureType", 2), ("usage", 1), ("arrayLength", 1)] {
            objc::bag_set(e, d, k, v);
        }
        send(e, d, "autorelease", &[])?;
        ret(e, d)
    }),
    ("IOSurface", false, "width", |e| surface_field(e, surface_width)),
    ("IOSurface", false, "height", |e| surface_field(e, surface_height)),
    ("IOSurface", false, "bytesPerRow", |e| surface_field(e, surface_bpr)),
    ("IOSurface", false, "allocationSize", |e| surface_field(e, surface_alloc)),
    ("IOSurface", false, "isInUse", |e| ret(e, 0)),
    ("IOSurface", false, "incrementUseCount", |_| Ok(())),
    ("IOSurface", false, "decrementUseCount", |_| Ok(())),
    (DEV, false, "newLibraryWithSource:options:error:", new_library_with_source),
    (DEV, false, "newLibraryWithData:error:", new_library_with_data),
    (DEV, false, "newLibraryWithSource:options:completionHandler:", new_library_async),
    (DEV, false, "newRenderPipelineStateWithDescriptor:error:", new_pipeline),
    (DEV, false, "newRenderPipelineStateWithDescriptor:completionHandler:", new_pipeline_async),
    (DEV, false, "newSamplerStateWithDescriptor:", new_sampler),
    (DEV, false, "newDepthStencilStateWithDescriptor:", new_depth_stencil),
    // A recent Apple GPU: every family through Apple 9, common (3) and Metal 3.
    (DEV, false, "supportsFamily:", |e| {
        let f = e.arg(2) as i64;
        let ok = (1001..=1009).contains(&f) || (3001..=3003).contains(&f) || f == 5001;
        ret(e, ok as u64)
    }),
    // MTLFeatureSet (the older capability API): every iOS set (< 10000), not macOS (10000+) or tvOS (30000+).
    (DEV, false, "supportsFeatureSet:", |e| {
        let f = e.arg(2);
        ret(e, (f < 10000) as u64)
    }),
    (DEV, false, "supportsTextureSampleCount:", |e| {
        let n = e.arg(2);
        ret(e, matches!(n, 1 | 2 | 4) as u64)
    }),
    (DEV, false, "maxBufferLength", |e| ret(e, 1 << 30)),
    (DEV, false, "maxThreadgroupMemoryLength", |e| ret(e, 32768)),
    (DEV, false, "recommendedMaxWorkingSetSize", |e| ret(e, 1 << 30)),
    (DEV, false, "hasUnifiedMemory", |e| ret(e, 1)),
    (DEV, false, "isLowPower", |e| ret(e, 0)),
    (DEV, false, "isHeadless", |e| ret(e, 0)),
    (DEV, false, "readWriteTextureSupport", |e| ret(e, 2)),
    (DEV, false, "argumentBuffersSupport", |e| ret(e, 1)),
    ("MTLPommeCommandQueue", false, "commandBuffer", new_command_buffer),
    ("MTLPommeCommandQueue", false, "commandBufferWithUnretainedReferences", new_command_buffer),
    ("MTLPommeCommandQueue", false, "commandBufferWithDescriptor:", new_command_buffer),
    ("MTLPommeCommandQueue", false, "device", |e| {
        let dev = e.metal.device;
        ret(e, dev)
    }),
    ("MTLPommeCommandBuffer", false, "renderCommandEncoderWithDescriptor:", render_encoder),
    ("MTLPommeCommandBuffer", false, "blitCommandEncoder", blit_encoder),
    ("MTLPommeCommandBuffer", false, "presentDrawable:", present_drawable),
    ("MTLPommeCommandBuffer", false, "addCompletedHandler:", add_completed_handler),
    // Scheduled and completed are the same moment here: the buffer runs at commit.
    ("MTLPommeCommandBuffer", false, "addScheduledHandler:", add_completed_handler),
    ("MTLPommeCommandBuffer", false, "commit", commit),
    ("MTLPommeCommandBuffer", false, "enqueue", |_| Ok(())),
    ("MTLPommeCommandBuffer", false, "waitUntilCompleted", |_| Ok(())),
    ("MTLPommeCommandBuffer", false, "waitUntilScheduled", |_| Ok(())),
    ("MTLPommeCommandBuffer", false, "status", |e| {
        let done = matches!(obj(e, e.arg(0)), Some(Obj::CommandBuffer { committed: true, .. }));
        ret(e, if done { 4 } else { 0 }) // MTLCommandBufferStatusCompleted / NotEnqueued
    }),
    ("MTLPommeCommandBuffer", false, "error", |e| ret(e, 0)),
    ("MTLPommeRenderCommandEncoder", false, "endEncoding", end_render_encoding),
    ("MTLPommeBlitCommandEncoder", false, "endEncoding", |e| {
        let enc = e.arg(0);
        e.metal.blits.remove(&enc);
        Ok(())
    }),
    (RE, false, "setRenderPipelineState:", |e| {
        let p = e.arg(2);
        if let Some(st) = enc_state(e) {
            st.pipeline = p;
        }
        Ok(())
    }),
    (RE, false, "setDepthStencilState:", |e| {
        let p = e.arg(2);
        if let Some(st) = enc_state(e) {
            st.depth_stencil = p;
        }
        Ok(())
    }),
    (RE, false, "setStencilReferenceValue:", |e| {
        let v = e.arg(2) as u32;
        if let Some(st) = enc_state(e) {
            st.stencil_ref = [v, v];
        }
        Ok(())
    }),
    (RE, false, "setStencilFrontReferenceValue:backReferenceValue:", |e| {
        let (f, b) = (e.arg(2) as u32, e.arg(3) as u32);
        if let Some(st) = enc_state(e) {
            st.stencil_ref = [f, b];
        }
        Ok(())
    }),
    (RE, false, "setScissorRect:", |e| {
        let p = e.arg(2);
        let r = [e.read_u64(p)?, e.read_u64(p + 8)?, e.read_u64(p + 16)?, e.read_u64(p + 24)?];
        if let Some(st) = enc_state(e) {
            st.scissor = Some(r);
        }
        Ok(())
    }),
    (RE, false, "setViewport:", |e| {
        let p = e.arg(2);
        let mut v = [0f64; 6];
        for (i, x) in v.iter_mut().enumerate() {
            *x = f64::from_bits(e.read_u64(p + i as u64 * 8)?);
        }
        if let Some(st) = enc_state(e) {
            st.viewport = Some(v);
        }
        Ok(())
    }),
    (RE, false, "setCullMode:", |e| {
        let v = e.arg(2);
        if let Some(st) = enc_state(e) {
            st.cull = v;
        }
        Ok(())
    }),
    (RE, false, "setFrontFacingWinding:", |e| {
        let v = e.arg(2);
        if let Some(st) = enc_state(e) {
            st.winding = v;
        }
        Ok(())
    }),
    (RE, false, "setBlendColorRed:green:blue:alpha:", |e| {
        let c = [0, 1, 2, 3].map(|i| f32::from_bits(e.cpu.q[i] as u32) as f64);
        if let Some(st) = enc_state(e) {
            st.blend_color = c;
        }
        Ok(())
    }),
    (RE, false, "setTriangleFillMode:", |_| Ok(())),
    (RE, false, "setVertexBuffer:offset:atIndex:", |e| set_buffer(e, 0)),
    (RE, false, "setFragmentBuffer:offset:atIndex:", |e| set_buffer(e, 1)),
    (RE, false, "setVertexBufferOffset:atIndex:", |e| set_buffer_offset(e, 0)),
    (RE, false, "setFragmentBufferOffset:atIndex:", |e| set_buffer_offset(e, 1)),
    (RE, false, "setVertexBytes:length:atIndex:", |e| set_bytes(e, 0)),
    (RE, false, "setFragmentBytes:length:atIndex:", |e| set_bytes(e, 1)),
    (RE, false, "setVertexTexture:atIndex:", |e| set_texture(e, 0)),
    (RE, false, "setFragmentTexture:atIndex:", |e| set_texture(e, 1)),
    (RE, false, "setVertexSamplerState:atIndex:", |e| set_sampler(e, 0)),
    (RE, false, "setFragmentSamplerState:atIndex:", |e| set_sampler(e, 1)),
    (RE, false, "drawPrimitives:vertexStart:vertexCount:", |e| draw_arrays(e, false, false)),
    (RE, false, "drawPrimitives:vertexStart:vertexCount:instanceCount:", |e| draw_arrays(e, true, false)),
    (RE, false, "drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:", |e| draw_arrays(e, true, true)),
    (RE, false, "drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:", |e| draw_indexed(e, false, false)),
    (RE, false, "drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:instanceCount:", |e| draw_indexed(e, true, false)),
    (RE, false, "drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:instanceCount:baseVertex:baseInstance:", |e| draw_indexed(e, true, true)),
    (BE, false, "copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:", |e| {
        let cmd = Cmd::CopyBuffer { src: e.arg(2), src_offset: e.arg(3), dst: e.arg(4), dst_offset: e.arg(5), size: e.arg(6) };
        blit_cmd(e, cmd);
        Ok(())
    }),
    (BE, false, "copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:", |e| {
        let size = triple(e, e.arg(6))?;
        let origin_ptr = e.read_u64(e.cpu.sp + 16)?;
        let origin = triple(e, origin_ptr)?;
        let cmd = Cmd::BufferToTexture { buffer: e.arg(2), offset: e.arg(3), bytes_per_row: e.arg(4), texture: e.arg(7), origin: [origin[0], origin[1]], size: [size[0], size[1]] };
        blit_cmd(e, cmd);
        Ok(())
    }),
    (BE, false, "copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:", |e| {
        let (so, sz) = (triple(e, e.arg(5))?, triple(e, e.arg(6))?);
        let dptr = e.read_u64(e.cpu.sp + 16)?;
        let d = triple(e, dptr)?;
        let cmd = Cmd::TextureToTexture { src: e.arg(2), src_origin: [so[0], so[1]], size: [sz[0], sz[1]], dst: e.arg(7), dst_origin: [d[0], d[1]] };
        blit_cmd(e, cmd);
        Ok(())
    }),
    (BE, false, "fillBuffer:range:value:", |e| {
        let (b, loc, len, v) = (e.arg(2), e.arg(3), e.arg(4), e.arg(5) as u8);
        if let Some(Obj::Buffer { contents, .. }) = obj(e, b).cloned() {
            e.p.memory.write(contents + loc, &vec![v; len as usize])?;
        }
        Ok(())
    }),
    (BE, false, "synchronizeResource:", |_| Ok(())),
    (BE, false, "generateMipmapsForTexture:", |_| Ok(())),
    ("MTLPommeTexture", false, "replaceRegion:mipmapLevel:withBytes:bytesPerRow:", replace_region),
    ("MTLRenderPassColorAttachmentDescriptor", false, "setClearColor:", |e| {
        let (a, c) = (e.arg(0), [d(e, 0), d(e, 1), d(e, 2), d(e, 3)]);
        e.metal.clear_colors.insert(a, c);
        Ok(())
    }),
    ("MTLRenderPassColorAttachmentDescriptor", false, "clearColor", |e| {
        let c = e.metal.clear_colors.get(&e.arg(0)).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]);
        for (i, v) in c.iter().enumerate() {
            e.cpu.q[i] = v.to_bits() as u128;
        }
        Ok(())
    }),
    ("MTLPommeBuffer", false, "contents", buffer_contents),
    ("MTLPommeBuffer", false, "length", buffer_length),
    ("MTLPommeBuffer", false, "didModifyRange:", |_| Ok(())),
    ("MTLPommeBuffer", false, "storageMode", |e| ret(e, 0)), // shared
    ("MTLPommeTexture", false, "width", |e| texture_dims(e, 0)),
    ("MTLPommeTexture", false, "height", |e| texture_dims(e, 1)),
    ("MTLPommeTexture", false, "pixelFormat", |e| texture_dims(e, 2)),
    ("MTLPommeTexture", false, "usage", |e| texture_dims(e, 3)),
    ("MTLPommeTexture", false, "depth", |e| ret(e, 1)),
    ("MTLPommeTexture", false, "mipmapLevelCount", |e| ret(e, 1)),
    ("MTLPommeTexture", false, "arrayLength", |e| ret(e, 1)),
    ("MTLPommeTexture", false, "sampleCount", |e| texture_dims(e, 4)),
    ("MTLPommeTexture", false, "textureType", |e| texture_dims(e, 5)),
    ("MTLPommeTexture", false, "isFramebufferOnly", |e| ret(e, 0)),
    ("MTLPommeTexture", false, "storageMode", |e| ret(e, 2)), // private
    ("MTLPommeLibrary", false, "newFunctionWithName:", new_function),
    // MTLFunctionType: vertex 1, fragment 2, kernel 3 (from the MSL entry point's qualifier).
    ("MTLPommeFunction", false, "functionType", |e| {
        let t = match obj(e, e.arg(0)).cloned() {
            Some(Obj::Function { name, library }) => match obj(e, library) {
                Some(Obj::Library { source, .. }) => {
                    let has = |q: &str| source.lines().any(|l| l.trim_start().starts_with(q) && l.contains(&format!(" {name}(")));
                    if has("vertex ") {
                        1
                    } else if has("fragment ") {
                        2
                    } else if has("kernel ") {
                        3
                    } else {
                        0
                    }
                }
                _ => 0,
            },
            _ => 0,
        };
        ret(e, t)
    }),
    ("MTLPommeFunction", false, "name", |e| {
        let n = match obj(e, e.arg(0)) {
            Some(Obj::Function { name, .. }) => name.clone(),
            _ => String::new(),
        };
        let s = foundation::new_nsstring(e, &n)?;
        ret(e, s)
    }),
    ("MTLPommeLibrary", false, "functionNames", |e| {
        let names: Vec<String> = match obj(e, e.arg(0)) {
            Some(Obj::BinaryLibrary { functions }) => functions.keys().cloned().collect(),
            _ => Vec::new(),
        };
        let mut objs = Vec::new();
        for n in names {
            objs.push(foundation::new_nsstring(e, &n)?);
        }
        let a = foundation::new_array_of(e, &objs)?;
        ret(e, a)
    }),
    ("MTLPommeLibrary", false, "newFunctionWithName:constantValues:error:", |e| {
        let (lib, name, cv) = (e.arg(0), e.arg(2), e.arg(3));
        let f = make_function(e, lib, name, cv)?;
        ret(e, f)
    }),
    ("MTLPommeLibrary", false, "newFunctionWithName:constantValues:completionHandler:", |e| {
        let (lib, name, cv, block) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let f = make_function(e, lib, name, cv)?;
        crate::libsystem::call_block(e, block, &[f, 0])?;
        Ok(())
    }),
    ("MTLPommeLibrary", false, "newFunctionWithName:completionHandler:", |e| {
        let (lib, name, block) = (e.arg(0), e.arg(2), e.arg(3));
        let f = make_function(e, lib, name, 0)?;
        crate::libsystem::call_block(e, block, &[f, 0])?;
        Ok(())
    }),
    // MTLDataType: Float 3, Half 16, Int 29, UInt 33, Bool 53.
    ("MTLFunctionConstantValues", false, "setConstantValue:type:atIndex:", |e| {
        let (cv, p, ty, i) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4) as u32);
        let v = match ty {
            3 => f32::from_bits(e.p.memory.read_u32(p)?) as f64,
            16 => crate::metal::half_to_f64(e.p.memory.read_u16(p)?),
            29 => e.p.memory.read_u32(p)? as i32 as f64,
            33 => e.p.memory.read_u32(p)? as f64,
            53 => e.p.memory.read_u8(p)? as f64,
            other => {
                println!("[metal] function constant {i}: unsupported MTLDataType {other}");
                0.0
            }
        };
        let list = e.metal.constant_values.entry(cv).or_default();
        list.retain(|&(k, _)| k != i);
        list.push((i, v));
        Ok(())
    }),
    ("MTLFunctionConstantValues", false, "reset", |e| {
        let cv = e.arg(0);
        e.metal.constant_values.remove(&cv);
        Ok(())
    }),
    ("CAMetalDrawable", false, "texture", drawable_texture),
    ("CAMetalDrawable", false, "layer", |e| {
        let l = match obj(e, e.arg(0)) {
            Some(Obj::Drawable { layer, .. }) => *layer,
            _ => 0,
        };
        ret(e, l)
    }),
    ("CAMetalDrawable", false, "present", |_| Ok(())),
    (ML, false, "nextDrawable", next_drawable),
    (ML, false, "setDevice:", |e| {
        let (l, dev) = (e.arg(0), e.arg(2));
        metal_layer(e, l).device = dev;
        Ok(())
    }),
    (ML, false, "device", |e| {
        let l = e.arg(0);
        let dev = metal_layer(e, l).device;
        ret(e, dev)
    }),
    (ML, false, "setPixelFormat:", |e| {
        let (l, f) = (e.arg(0), e.arg(2));
        metal_layer(e, l).pixel_format = f;
        Ok(())
    }),
    (ML, false, "pixelFormat", |e| {
        let l = e.arg(0);
        let f = metal_layer(e, l).pixel_format;
        ret(e, f)
    }),
    (ML, false, "setDrawableSize:", |e| {
        let (l, w, h) = (e.arg(0), d(e, 0), d(e, 1));
        metal_layer(e, l).drawable_size = [w, h];
        Ok(())
    }),
    (ML, false, "drawableSize", |e| {
        let l = e.arg(0);
        let s = metal_layer(e, l).drawable_size;
        e.cpu.q[0] = s[0].to_bits() as u128;
        e.cpu.q[1] = s[1].to_bits() as u128;
        Ok(())
    }),
    (ML, false, "setFramebufferOnly:", |e| {
        let (l, v) = (e.arg(0), e.arg(2) & 0xff != 0);
        metal_layer(e, l).framebuffer_only = v;
        Ok(())
    }),
    (ML, false, "setColorspace:", |_| Ok(())),
    (ML, false, "setPresentsWithTransaction:", |_| Ok(())),
    (ML, false, "setMaximumDrawableCount:", |_| Ok(())),
    (ML, false, "setWantsExtendedDynamicRangeContent:", |_| Ok(())),
];

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_MTLCreateSystemDefaultDevice", system_default_device),
    ("_IOSurfaceCreate", iosurface_create),
    ("_IOSurfaceAlignProperty", iosurface_align),
    ("_IOSurfaceSetValue", |_| Ok(())),
    ("_IOSurfaceGetWidth", |e| surface_field(e, surface_width)),
    ("_IOSurfaceGetHeight", |e| surface_field(e, surface_height)),
    ("_IOSurfaceGetBytesPerRow", |e| surface_field(e, surface_bpr)),
    ("_IOSurfaceGetAllocSize", |e| surface_field(e, surface_alloc)),
    ("_IOSurfaceIsInUse", |e| ret(e, 0)),
    ("_IOSurfaceIncrementUseCount", |_| Ok(())),
    ("_IOSurfaceDecrementUseCount", |_| Ok(())),
    // CVMetalTextureCache (external textures: video, camera). A cache object
    // exists; making textures from pixel buffers isn't supported yet.
    ("_CVMetalTextureCacheCreate", |e| {
        let out = e.arg(4);
        let cache = foundation::new_array_of(e, &[])?;
        if out != 0 {
            e.write_u64(out, cache)?;
        }
        ret(e, 0) // kCVReturnSuccess
    }),
    ("_CVMetalTextureCacheFlush", |_| Ok(())),
];
