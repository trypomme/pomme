#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
struct Inputs {
    float4 fillBounds  [[attribute(0)]];
    half4 color  [[attribute(1)]];
    float4 locations  [[attribute(2)]];
};
struct Outputs {
    float4 sk_Position [[position]];
    float2 vatlasCoord_S0 [[user(locn0)]];
    half4 vcolor_S0 [[user(locn1) flat]];
    float sk_PointSize [[point_size]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
float2 uatlas_adjust_S0;
};
struct Globals {
    constant uniformBuffer* _anonInterface0;
};
vertex Outputs vertexMain(Inputs _in [[stage_in]], constant uniformBuffer& _anonInterface0 [[buffer(0)]], uint sk_VertexID [[vertex_id]], uint sk_InstanceID [[instance_id]]) {
Globals _globals{&_anonInterface0};
(void)_globals;
Outputs _out;
(void)_out;
float2 unitCoord = float2(float(sk_VertexID & 1), float(sk_VertexID >> 1));
float2 devCoord = mix(_in.fillBounds.xy, _in.fillBounds.zw, unitCoord);
float2 atlasTopLeft = float2(abs(_in.locations.x) - 1.0, _in.locations.y);
float2 devTopLeft = _in.locations.zw;
bool transposed = _in.locations.x < 0.0;
float2 atlasCoord = devCoord - devTopLeft;
if (transposed) {
atlasCoord = atlasCoord.yx;
}
atlasCoord += atlasTopLeft;
_out.vatlasCoord_S0 = atlasCoord * _globals._anonInterface0->uatlas_adjust_S0;
_out.vcolor_S0 = _in.color;
_out.sk_Position = float4(devCoord, 0.0, 1.0);
_out.sk_Position = float4(_out.sk_Position.xy * _globals._anonInterface0->sk_RTAdjust.xz + _out.sk_Position.ww * _globals._anonInterface0->sk_RTAdjust.yw, 0.0, _out.sk_Position.w);
return _out;
}
