#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FrameInfo
{
    float4x4 mvp;
};

struct imp_line_vertex_main_out
{
    float2 v_position [[user(locn0)]];
    float3 v_e0 [[user(locn1)]];
    float3 v_e1 [[user(locn2)]];
    float3 v_e2 [[user(locn3)]];
    float3 v_e3 [[user(locn4)]];
    float4 gl_Position [[position]];
};

struct imp_line_vertex_main_in
{
    float2 position [[attribute(0)]];
    float3 e0 [[attribute(1)]];
    float3 e1 [[attribute(2)]];
    float3 e2 [[attribute(3)]];
    float3 e3 [[attribute(4)]];
};

vertex imp_line_vertex_main_out imp_line_vertex_main(imp_line_vertex_main_in in [[stage_in]], constant FrameInfo& frame_info [[buffer(0)]])
{
    imp_line_vertex_main_out out = {};
    out.gl_Position = frame_info.mvp * float4(in.position, 0.0, 1.0);
    out.v_position = in.position;
    out.v_e0 = in.e0;
    out.v_e1 = in.e1;
    out.v_e2 = in.e2;
    out.v_e3 = in.e3;
    return out;
}

