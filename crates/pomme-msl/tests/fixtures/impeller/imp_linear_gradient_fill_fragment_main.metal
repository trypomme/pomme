#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float2 start_point;
    float2 end_point;
    float tile_mode;
    float4 decal_border_color;
    float alpha;
    float2 half_texel;
};

struct imp_linear_gradient_fill_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_linear_gradient_fill_fragment_main_in
{
    float2 v_position [[user(locn0)]];
};

fragment imp_linear_gradient_fill_fragment_main_out imp_linear_gradient_fill_fragment_main(imp_linear_gradient_fill_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_linear_gradient_fill_fragment_main_out out = {};
    float2 _243 = frag_info.end_point - frag_info.start_point;
    float _258 = dot(in.v_position - frag_info.start_point, _243) / dot(_243, _243);
    float4 _517;
    do
    {
        bool _325 = frag_info.tile_mode == 3.0;
        bool _338;
        if (_325)
        {
            bool _329 = _258 < 0.0;
            bool _336;
            if (!_329)
            {
                _336 = _258 >= 1.0;
            }
            else
            {
                _336 = _329;
            }
            _338 = _336;
        }
        else
        {
            _338 = _325;
        }
        bool _357;
        if (!_338)
        {
            _357 = _325 ? false : _325;
        }
        else
        {
            _357 = _338;
        }
        if (_357)
        {
            _517 = frag_info.decal_border_color;
            break;
        }
        bool _389 = frag_info.tile_mode == 0.0;
        float _511;
        if (_389)
        {
            _511 = fast::clamp(_258, 0.0, 1.0);
        }
        else
        {
            float _512;
            if (frag_info.tile_mode == 1.0)
            {
                _512 = fract(_258);
            }
            else
            {
                float _513;
                if (frag_info.tile_mode == 2.0)
                {
                    float _404 = _258 - 1.0;
                    _513 = abs((_404 - (2.0 * floor(_404 * 0.5))) - 1.0);
                }
                else
                {
                    _513 = _258;
                }
                _512 = _513;
            }
            _511 = _512;
        }
        float _514;
        if (_389)
        {
            _514 = 0.5;
        }
        else
        {
            float _515;
            if (frag_info.tile_mode == 1.0)
            {
                _515 = fract(0.5);
            }
            else
            {
                float _516;
                if (frag_info.tile_mode == 2.0)
                {
                    _516 = abs((-1.5) - (2.0 * floor(-0.25)));
                }
                else
                {
                    _516 = 0.5;
                }
                _515 = _516;
            }
            _514 = _515;
        }
        _517 = texture_sampler.sample(texture_samplerSmplr, float2(mix(frag_info.half_texel.x, 1.0 - frag_info.half_texel.x, _511), mix(frag_info.half_texel.y, 1.0 - frag_info.half_texel.y, _514)), bias(-0.4749999940395355224609375));
        break;
    } while(false);
    out.frag_color = _517;
    out.frag_color = float4(out.frag_color.xyz * out.frag_color.w, out.frag_color.w) * frag_info.alpha;
    return out;
}

