#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FrameInfo
{
    float4x4 mvp;
    float4x4 uv_transform;
};

struct imp_texture_uv_fill_vertex_main_out
{
    float2 v_texture_coords [[user(locn0)]];
    float4 gl_Position [[position]];
};

struct imp_texture_uv_fill_vertex_main_in
{
    float2 position [[attribute(0)]];
};

vertex imp_texture_uv_fill_vertex_main_out imp_texture_uv_fill_vertex_main(imp_texture_uv_fill_vertex_main_in in [[stage_in]], constant FrameInfo& frame_info [[buffer(0)]])
{
    imp_texture_uv_fill_vertex_main_out out = {};
    float4 _33 = float4(in.position, 0.0, 1.0);
    out.gl_Position = frame_info.mvp * _33;
    out.v_texture_coords = (frame_info.uv_transform * _33).xy;
    return out;
}

