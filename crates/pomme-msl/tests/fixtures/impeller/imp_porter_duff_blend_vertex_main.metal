#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FrameInfo
{
    float4x4 mvp;
};

struct imp_porter_duff_blend_vertex_main_out
{
    float2 v_texture_coords [[user(locn0)]];
    float4 v_color [[user(locn1)]];
    float4 gl_Position [[position]];
};

struct imp_porter_duff_blend_vertex_main_in
{
    float2 vertices [[attribute(0)]];
    float2 texture_coords [[attribute(1)]];
    float4 color [[attribute(2)]];
};

vertex imp_porter_duff_blend_vertex_main_out imp_porter_duff_blend_vertex_main(imp_porter_duff_blend_vertex_main_in in [[stage_in]], constant FrameInfo& frame_info [[buffer(0)]])
{
    imp_porter_duff_blend_vertex_main_out out = {};
    out.gl_Position = frame_info.mvp * float4(in.vertices, 0.0, 1.0);
    out.v_color = in.color;
    out.v_texture_coords = in.texture_coords;
    return out;
}

