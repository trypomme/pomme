#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FrameInfo
{
    float4x4 mvp;
};

struct imp_filter_position_vertex_main_out
{
    float2 v_texture_coords [[user(locn0)]];
    float4 gl_Position [[position]];
};

struct imp_filter_position_vertex_main_in
{
    float2 position [[attribute(0)]];
};

vertex imp_filter_position_vertex_main_out imp_filter_position_vertex_main(imp_filter_position_vertex_main_in in [[stage_in]], constant FrameInfo& frame_info [[buffer(0)]])
{
    imp_filter_position_vertex_main_out out = {};
    out.gl_Position = frame_info.mvp * float4(in.position, 0.0, 1.0);
    out.v_texture_coords = in.position;
    return out;
}

