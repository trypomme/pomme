#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FrameInfo
{
    float4x4 mvp;
    float4x4 matrix;
};

struct imp_gradient_fill_vertex_main_out
{
    float2 v_position [[user(locn0)]];
    float4 gl_Position [[position]];
};

struct imp_gradient_fill_vertex_main_in
{
    float2 position [[attribute(0)]];
};

vertex imp_gradient_fill_vertex_main_out imp_gradient_fill_vertex_main(imp_gradient_fill_vertex_main_in in [[stage_in]], constant FrameInfo& frame_info [[buffer(0)]])
{
    imp_gradient_fill_vertex_main_out out = {};
    float4 _59 = float4(in.position, 0.0, 1.0);
    out.gl_Position = frame_info.mvp * _59;
    float4 _83 = frame_info.matrix * _59;
    out.v_position = _83.xy / float2(_83.w);
    return out;
}

