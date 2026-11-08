#include <metal_stdlib>
#include <simd/simd.h>
#ifdef __clang__
#pragma clang diagnostic ignored "-Wall"
#endif
using namespace metal;
struct Inputs {
};
struct Outputs {
    half4 sk_FragColor [[color(0)]];
};
struct uniformBuffer {
float4 sk_RTAdjust;
float4 uaffineMatrix_S0;
float2 utranslate_S0;
half4 ucolor_S0;
};
struct Globals {
    constant uniformBuffer* _anonInterface0;
};
fragment Outputs fragmentMain(Inputs _in [[stage_in]], constant uniformBuffer& _anonInterface0 [[buffer(0)]], bool _frontFacing [[front_facing]], float4 _fragCoord [[position]]) {
Globals _globals{&_anonInterface0};
(void)_globals;
Outputs _out;
(void)_out;
return _out;
}
