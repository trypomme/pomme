#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FrameInfo
{
    float4x4 mvp;
};

struct imp_texture_fill_vertex_main_out
{
    float2 v_texture_coords [[user(locn0)]];
    float4 gl_Position [[position]];
};

struct imp_texture_fill_vertex_main_in
{
    float2 position [[attribute(0)]];
    float2 texture_coords [[attribute(1)]];
};

vertex imp_texture_fill_vertex_main_out imp_texture_fill_vertex_main(imp_texture_fill_vertex_main_in in [[stage_in]], constant FrameInfo& frame_info [[buffer(0)]])
{
    imp_texture_fill_vertex_main_out out = {};
    out.gl_Position = frame_info.mvp * float4(in.position, 0.0, 1.0);
    out.v_texture_coords = in.texture_coords;
    return out;
}

