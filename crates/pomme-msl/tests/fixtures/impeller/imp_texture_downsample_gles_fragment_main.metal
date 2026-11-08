#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct FragInfo
{
    float edge;
    float ratio;
    float2 pixel_size;
};

struct imp_texture_downsample_gles_fragment_main_out
{
    float4 frag_color [[color(0)]];
};

struct imp_texture_downsample_gles_fragment_main_in
{
    float2 v_texture_coords [[user(locn0)]];
};

fragment imp_texture_downsample_gles_fragment_main_out imp_texture_downsample_gles_fragment_main(imp_texture_downsample_gles_fragment_main_in in [[stage_in]], constant FragInfo& frag_info [[buffer(0)]], texture2d<float> texture_sampler [[texture(0)]], sampler texture_samplerSmplr [[sampler(0)]])
{
    imp_texture_downsample_gles_fragment_main_out out = {};
    float _32 = -frag_info.edge;
    float4 _182;
    _182 = float4(0.0);
    float4 _190;
    for (float _181 = _32; _181 <= frag_info.edge; _182 = _190, _181 += 2.0)
    {
        _190 = _182;
        float4 _75;
        for (float _183 = _32; _183 <= frag_info.edge; _190 = _75, _183 += 2.0)
        {
            float2 _67 = in.v_texture_coords + (frag_info.pixel_size * float2(_181, _183));
            float4 _185;
            do
            {
                float _146 = _67.x;
                bool _147 = _146 < 0.0;
                bool _154;
                if (!_147)
                {
                    _154 = _67.y < 0.0;
                }
                else
                {
                    _154 = _147;
                }
                bool _161;
                if (!_154)
                {
                    _161 = _146 > 1.0;
                }
                else
                {
                    _161 = _154;
                }
                bool _168;
                if (!_161)
                {
                    _168 = _67.y > 1.0;
                }
                else
                {
                    _168 = _161;
                }
                if (_168)
                {
                    _185 = float4(0.0);
                    break;
                }
                else
                {
                    _185 = texture_sampler.sample(texture_samplerSmplr, _67, bias(-0.4749999940395355224609375));
                    break;
                }
                break; // unreachable workaround
            } while(false);
            _75 = _190 + (_185 * frag_info.ratio);
        }
    }
    out.frag_color = _182;
    return out;
}

