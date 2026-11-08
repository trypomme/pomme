#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FrameInfo
{
    float4x4 mvp;
    float depth;
};

struct imp_clip_vertex_main_out
{
    float4 gl_Position [[position]];
};

struct imp_clip_vertex_main_in
{
    float2 position [[attribute(0)]];
};

vertex imp_clip_vertex_main_out imp_clip_vertex_main(imp_clip_vertex_main_in in [[stage_in]], constant FrameInfo& frame_info [[buffer(0)]])
{
    imp_clip_vertex_main_out out = {};
    out.gl_Position = frame_info.mvp * float4(in.position, 0.0, 1.0);
    out.gl_Position.z = frame_info.depth * out.gl_Position.w;
    return out;
}

