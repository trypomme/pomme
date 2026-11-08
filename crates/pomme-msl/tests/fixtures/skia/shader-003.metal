#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
struct Inputs {
    half4 vcolor_S0 [[user(locn0) flat]];
};
struct Outputs {
    half4 sk_FragColor [[color(0)]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
};
struct Globals {
    constant uniformBuffer* _anonInterface0;
};
fragment Outputs fragmentMain(Inputs _in [[stage_in]], constant uniformBuffer& _anonInterface0 [[buffer(0)]], bool _frontFacing [[front_facing]], float4 _fragCoord [[position]]) {
Globals _globals{&_anonInterface0};
(void)_globals;
Outputs _out;
(void)_out;
half4 outputColor_S0 = _in.vcolor_S0;
{
_out.sk_FragColor = outputColor_S0;
_out.sk_FragColor = half4(_out.sk_FragColor.w, 0.0h, 0.0h, 0.0h);
}
return _out;
}
