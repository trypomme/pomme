//! Metal: devices, queues, command buffers and encoders, buffers, textures,
//! shader libraries and pipelines, and CAMetalLayer drawables.
//!
//! This is the bring-up stage. Objects behave like Metal's (buffers' contents
//! live in guest memory, descriptors are property bags, encoders record what
//! the app asks for), shader sources are captured (`POMME_DUMP_MSL=dir`), and
//! nothing is executed yet: execution on wgpu, with MSL translated to naga IR,
//! comes next.

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
    Library { source: String },
    Function { name: String, library: u64 },
    Pipeline { descriptor: u64 },
    Sampler,
    DepthStencil,
    Drawable { layer: u64, texture: u64 },
}

#[derive(Debug, Clone, Default)]
pub struct Texture {
    pub width: u64,
    pub height: u64,
    pub format: u64,
    pub usage: u64,
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
    /// Drawables presented by committed command buffers.
    pub presented: u64,
    device: u64,
    /// Descriptor arrays → the class of their elements.
    element_class: HashMap<u64, &'static str>,
}

#[derive(Debug, Clone, Default)]
pub struct MetalLayer {
    pub device: u64,
    pub pixel_format: u64,
    pub drawable_size: [f64; 2],
    pub framebuffer_only: bool,
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
    let t = Texture { width: get(emu, "width"), height: get(emu, "height"), format: get(emu, "pixelFormat"), usage: get(emu, "usage") };
    trace(emu, || format!("texture {}×{} format {}", t.width, t.height, t.format));
    let o = new_obj(emu, "MTLPommeTexture", Obj::Texture(t))?;
    ret(emu, o)
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
    let lib = new_obj(emu, "MTLPommeLibrary", Obj::Library { source })?;
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

fn new_function(emu: &mut Emu) -> Result<()> {
    let (lib, name) = (emu.arg(0), emu.arg(2));
    let name = foundation::nsstring(emu, name)?;
    let f = new_obj(emu, "MTLPommeFunction", Obj::Function { name, library: lib })?;
    ret(emu, f)
}

fn new_pipeline(emu: &mut Emu) -> Result<()> {
    let desc = emu.arg(2);
    send(emu, desc, "retain", &[])?;
    let p = new_obj(emu, "MTLPommeRenderPipelineState", Obj::Pipeline { descriptor: desc })?;
    let names = |emu: &mut Emu, key: &str| -> String {
        let f = objc::bag_get(emu, desc, key).unwrap_or(0);
        match obj(emu, f) {
            Some(Obj::Function { name, .. }) => name.clone(),
            _ => "-".into(),
        }
    };
    let (v, f) = (names(emu, "vertexFunction"), names(emu, "fragmentFunction"));
    trace(emu, || format!("pipeline {p:#x}: {v} / {f}"));
    ret(emu, p)
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

fn render_encoder(emu: &mut Emu) -> Result<()> {
    let (cb, pass) = (emu.arg(0), emu.arg(2));
    send(emu, pass, "retain", &[])?;
    let e = new_obj(emu, "MTLPommeRenderCommandEncoder", Obj::RenderEncoder { buffer: cb, pass })?;
    count(emu, "renderCommandEncoderWithDescriptor:");
    send(emu, e, "autorelease", &[])?;
    ret(emu, e)
}

fn blit_encoder(emu: &mut Emu) -> Result<()> {
    let cb = emu.arg(0);
    let e = new_obj(emu, "MTLPommeBlitCommandEncoder", Obj::BlitEncoder { buffer: cb })?;
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
    emu.metal.presented += present.len() as u64;
    for h in handlers {
        crate::libsystem::call_block(emu, h, &[cb])?;
    }
    Ok(())
}

/// Encoder calls that just get counted for now.
fn record(emu: &mut Emu) -> Result<()> {
    let sel = objc::sel_name(emu, emu.cpu.x[1]);
    count(emu, &sel);
    Ok(())
}

// ---- CAMetalLayer ----

fn metal_layer(emu: &mut Emu, l: u64) -> &mut MetalLayer {
    emu.metal.layers.entry(l).or_insert_with(|| MetalLayer { pixel_format: BGRA8, framebuffer_only: true, ..Default::default() })
}

fn next_drawable(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let ml = metal_layer(emu, l).clone();
    let t = Texture { width: ml.drawable_size[0] as u64, height: ml.drawable_size[1] as u64, format: ml.pixel_format, usage: 4 };
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
        Some(Obj::Texture(t)) => [t.width, t.height, t.format, t.usage][which],
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
    (DEV, false, "newLibraryWithSource:options:error:", new_library_with_source),
    (DEV, false, "newLibraryWithSource:options:completionHandler:", new_library_async),
    (DEV, false, "newRenderPipelineStateWithDescriptor:error:", new_pipeline),
    (DEV, false, "newRenderPipelineStateWithDescriptor:completionHandler:", new_pipeline_async),
    (DEV, false, "newSamplerStateWithDescriptor:", |e| {
        let s = new_obj(e, "MTLPommeSamplerState", Obj::Sampler)?;
        ret(e, s)
    }),
    (DEV, false, "newDepthStencilStateWithDescriptor:", |e| {
        let s = new_obj(e, "MTLPommeDepthStencilState", Obj::DepthStencil)?;
        ret(e, s)
    }),
    // A recent Apple GPU: every family through Apple 9, common (3) and Metal 3.
    (DEV, false, "supportsFamily:", |e| {
        let f = e.arg(2) as i64;
        let ok = (1001..=1009).contains(&f) || (3001..=3003).contains(&f) || f == 5001;
        ret(e, ok as u64)
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
    ("MTLPommeCommandBuffer", false, "addScheduledHandler:", |_| Ok(())),
    ("MTLPommeCommandBuffer", false, "commit", commit),
    ("MTLPommeCommandBuffer", false, "enqueue", |_| Ok(())),
    ("MTLPommeCommandBuffer", false, "waitUntilCompleted", |_| Ok(())),
    ("MTLPommeCommandBuffer", false, "waitUntilScheduled", |_| Ok(())),
    ("MTLPommeCommandBuffer", false, "status", |e| {
        let done = matches!(obj(e, e.arg(0)), Some(Obj::CommandBuffer { committed: true, .. }));
        ret(e, if done { 4 } else { 0 }) // MTLCommandBufferStatusCompleted / NotEnqueued
    }),
    ("MTLPommeCommandBuffer", false, "error", |e| ret(e, 0)),
    ("MTLPommeRenderCommandEncoder", false, "endEncoding", record),
    ("MTLPommeBlitCommandEncoder", false, "endEncoding", record),
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
    ("MTLPommeTexture", false, "sampleCount", |e| ret(e, 1)),
    ("MTLPommeTexture", false, "textureType", |e| ret(e, 2)), // 2D
    ("MTLPommeTexture", false, "isFramebufferOnly", |e| ret(e, 0)),
    ("MTLPommeTexture", false, "storageMode", |e| ret(e, 2)), // private
    ("MTLPommeLibrary", false, "newFunctionWithName:", new_function),
    ("MTLPommeLibrary", false, "newFunctionWithName:constantValues:error:", new_function),
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

pub(crate) const FUNCS: &[(&str, HostFn)] = &[("_MTLCreateSystemDefaultDevice", system_default_device)];
