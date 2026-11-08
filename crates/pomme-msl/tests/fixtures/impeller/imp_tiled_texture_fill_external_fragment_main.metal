#pragma clang diagnostic ignored "-Wmissing-prototypes"

#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

// Implementation of the GLSL mod() function, which is slightly different than Metal fmod()
template<typename Tx, typename Ty>
inline Tx mod(Tx x, Ty y)
{
    return x - y * floor(x / y);
}

struct FragInfo
{
    float x_tile_mode;
    float y_tile_mode;
    float alpha;
};

struct imp_tiled_texture_fill_external_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_tiled_texture_fill_external_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_tiled_texture_fill_external_fragment_main_out imp_tiled_texture_fill_external_fragment_main(imp_tiled_texture_fill_external_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> SAMPLER_EXTERNAL_OES_texture_sampler [[texture(0)]], sampler SAMPLER_EXTERNAL_OES_texture_samplerSmplr [[sampler(0)]])
{
    imp_tiled_texture_fill_external_fragment_main_out out = {};
    float2 _134 = in.v_texture_coords;
    float4 _277;
    do
    {
        bool _164 = frag_info.x_tile_mode == 3.0;
        bool _177;
        if (_164)
        {
            bool _168 = _134.x < 0.0;
            bool _175;
            if (!_168)
            {
                _175 = _134.x >= 1.0;
            }
            else
            {
                _175 = _168;
            }
            _177 = _175;
        }
        else
        {
            _177 = _164;
        }
        bool _196;
        if (!_177)
        {
            bool _181 = frag_info.y_tile_mode == 3.0;
            bool _194;
            if (_181)
            {
                bool _185 = _134.y < 0.0;
                bool _192;
                if (!_185)
                {
                    _192 = _134.y >= 1.0;
                }
                else
                {
                    _192 = _185;
                }
                _194 = _192;
            }
            else
            {
                _194 = _181;
            }
            _196 = _194;
        }
        else
        {
            _196 = _177;
        }
        if (_196)
        {
            _277 = float4(0.0);
            break;
        }
        float2 _273;
        if (frag_info.x_tile_mode == 1.0)
        {
            float2 _258 = _134;
            _258.x = mod(_134.x, 1.0);
            _273 = _258;
        }
        else
        {
            float2 _274;
            if (frag_info.x_tile_mode == 2.0)
            {
                float2 _261 = _134;
                _261.x = abs(mod(_134.x + 1.0, 2.0) - 1.0);
                _274 = _261;
            }
            else
            {
                _274 = _134;
            }
            _273 = _274;
        }
        float2 _275;
        if (frag_info.y_tile_mode == 1.0)
        {
            float2 _264 = _273;
            _264.y = mod(_273.y, 1.0);
            _275 = _264;
        }
        else
        {
            float2 _276;
            if (frag_info.y_tile_mode == 2.0)
            {
                float2 _267 = _273;
                _267.y = abs(mod(_273.y + 1.0, 2.0) - 1.0);
                _276 = _267;
            }
            else
            {
                _276 = _273;
            }
            _275 = _276;
        }
        _277 = SAMPLER_EXTERNAL_OES_texture_sampler.sample(SAMPLER_EXTERNAL_OES_texture_samplerSmplr, _275);
        break;
    } while(false);
    out.frag_color = _277 * frag_info.alpha;
    return out;
}

