#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
struct Inputs {
    float2 inPosition  [[attribute(0)]];
    half4 inColor  [[attribute(1)]];
    float4 inCircleEdge  [[attribute(2)]];
};
struct Outputs {
    float4 sk_Position [[position]];
    float4 vinCircleEdge_S0 [[user(locn0)]];
    half4 vinColor_S0 [[user(locn1)]];
    float sk_PointSize [[point_size]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
float4 ulocalMatrix_S0;
};
struct Globals {
    constant uniformBuffer* _anonInterface0;
};
vertex Outputs vertexMain(Inputs _in [[stage_in]], constant uniformBuffer& _anonInterface0 [[buffer(0)]], uint sk_VertexID [[vertex_id]], uint sk_InstanceID [[instance_id]]) {
Globals _globals{&_anonInterface0};
(void)_globals;
Outputs _out;
(void)_out;
_out.vinCircleEdge_S0 = _in.inCircleEdge;
_out.vinColor_S0 = _in.inColor;
float2 _tmp_0_inPosition = _in.inPosition;
_out.sk_Position = float4(_tmp_0_inPosition, 0.0, 1.0);
_out.sk_Position = float4(_out.sk_Position.xy * _globals._anonInterface0->sk_RTAdjust.xz + _out.sk_Position.ww * _globals._anonInterface0->sk_RTAdjust.yw, 0.0, _out.sk_Position.w);
return _out;
}
