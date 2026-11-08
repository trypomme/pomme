#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
struct Inputs {
    float2 position  [[attribute(0)]];
    half4 color  [[attribute(1)]];
};
struct Outputs {
    float4 sk_Position [[position]];
    half4 vcolor_S0 [[user(locn0) flat]];
    float sk_PointSize [[point_size]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
};
struct Globals {
    constant uniformBuffer* _anonInterface0;
};
vertex Outputs vertexMain(Inputs _in [[stage_in]], constant uniformBuffer& _anonInterface0 [[buffer(0)]], uint sk_VertexID [[vertex_id]], uint sk_InstanceID [[instance_id]]) {
Globals _globals{&_anonInterface0};
(void)_globals;
Outputs _out;
(void)_out;
_out.vcolor_S0 = _in.color;
_out.sk_Position = float4(_in.position, 0.0, 1.0);
_out.sk_Position = float4(_out.sk_Position.xy * _globals._anonInterface0->sk_RTAdjust.xz + _out.sk_Position.ww * _globals._anonInterface0->sk_RTAdjust.yw, 0.0, _out.sk_Position.w);
return _out;
}
