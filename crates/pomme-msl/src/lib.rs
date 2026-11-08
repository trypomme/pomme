//! Translate the Metal Shading Language that apps generate at runtime to WGSL,
//! so pomme can run their Metal work on wgpu.
//!
//! The target is generated MSL: Skia's Ganesh backend (SkSL → MSL) first,
//! Flutter's Impeller next. See `translate.rs` for how MSL's semantics
//! (implicit conversions, Metal buffer layouts, resource structs) map to WGSL.

mod lexer;
mod parser;
mod translate;

pub use translate::{Binding, EntryPoint, FRAMEBUFFER_BASE, SAMPLER_BASE, Scalar, Stage, TEXTURE_BASE, Translation, VertexInput, group_for};

/// Translate an MSL library's source to a WGSL module plus where each Metal
/// binding ended up.
pub fn translate(msl: &str) -> Result<Translation, String> {
    translate_specialized(msl, &Default::default())
}

/// Translate with function constants (`[[function_constant(n)]]`) set: index → value.
/// Constants without a value are undefined (`is_function_constant_defined` is false).
pub fn translate_specialized(msl: &str, constants: &std::collections::HashMap<u32, f64>) -> Result<Translation, String> {
    let items = parser::parse(msl)?;
    translate::translate(&items, constants)
}

#[cfg(test)]
mod tests {
    /// Uniforms are read at Metal's offsets: float4 a @0, float2 b @16, half4 c
    /// @24 (8 bytes, 8-aligned), float3x3 m @32 (three 16-byte columns), half h @80;
    /// the struct is 96 bytes (16-aligned).
    #[test]
    fn uniforms_load_at_metal_offsets() {
        let msl = r#"
            using namespace metal;
            struct U { float4 a; float2 b; half4 c; float3x3 m; half h; };
            struct Out { half4 color [[color(0)]]; };
            fragment Out main0(constant U& u [[buffer(2)]]) {
                Out o;
                o.color = u.c * u.h + half4(u.m[1], 1.0h) * u.a.x;
                return o;
            }
        "#;
        let t = crate::translate(msl).unwrap();
        let w = &t.wgsl;
        assert!(w.contains("_f16(extractBits(_w2(6u), 0u, 16u)), _f16(extractBits(_w2(6u), 16u, 16u)), _f16(extractBits(_w2(7u), 0u, 16u))"), "{w}");
        assert!(w.contains("vec3<f32>(bitcast<f32>(_w2(12u)), bitcast<f32>(_w2(13u)), bitcast<f32>(_w2(14u)))"), "{w}");
        assert!(w.contains("_f16(extractBits(_w2(20u), 0u, 16u))"), "{w}");
        let ep = &t.entry_points[0];
        assert_eq!(ep.buffers[0], crate::Binding { index: 2, group: 1, binding: 2, size: 96 });
        assert_eq!(ep.color_outputs, vec![0]);
    }

    /// A struct read through a `device` buffer ending in `[1]` is a runtime-sized
    /// array (Impeller's gradient stops): it gets room for more elements.
    #[test]
    fn device_buffer_arrays_have_room() {
        let msl = r#"
            using namespace metal;
            struct Stop { float4 color; float stop; };
            struct Stops { Stop colors[1]; };
            struct Out { float4 color [[color(0)]]; };
            fragment Out main0(const device Stops& s [[buffer(1)]], constant int& n [[buffer(0)]]) {
                Out o;
                o.color = s.colors[n - 1].color;
                return o;
            }
        "#;
        let t = crate::translate(msl).unwrap();
        let cap = crate::translate::DEVICE_ARRAY_CAPACITY;
        assert!(t.wgsl.contains(&format!("array<Stop, {cap}>")), "{}", t.wgsl);
        let b = t.entry_points[0].buffers.iter().find(|b| b.index == 1).unwrap();
        assert_eq!(b.size, 32 * cap);
    }
}
