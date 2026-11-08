// CPU conformance fixture: each function is called from tests/run.rs and its
// result compared with a Rust reference implementation.
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>

uint64_t fib(uint32_t n) {
    uint64_t a = 0, b = 1;
    for (uint32_t i = 0; i < n; i++) { uint64_t t = a + b; a = b; b = t; }
    return a;
}

int64_t sum_squares(int32_t n) {
    int32_t buf[64];
    for (int32_t i = 0; i < n && i < 64; i++) buf[i] = i - 20;
    int64_t s = 0;
    for (int32_t i = 0; i < n && i < 64; i++) s += (int64_t)buf[i] * buf[i];
    return s;
}

uint32_t popcount_loop(uint64_t x) {
    uint32_t c = 0;
    while (x) { x &= x - 1; c++; }
    return c;
}

int64_t divmod(int64_t a, int64_t b) { return (a / b) * 1000 + (a % b); }
uint32_t udivmod32(uint32_t a, uint32_t b) { return (a / b) * 1000u + (a % b); }

uint64_t mulhi(uint64_t a, uint64_t b) { return (uint64_t)(((__uint128_t)a * b) >> 64); }
int64_t smulhi(int64_t a, int64_t b) { return (int64_t)(((__int128_t)a * b) >> 64); }

int classify(int x) {
    switch (x) {
    case 0: return 11; case 1: return 23; case 2: return 37; case 3: return 41;
    case 4: return 53; case 5: return 67; case 6: return 71; case 7: return 89;
    case 8: return 97; case 9: return 101; default: return -1;
    }
}

uint64_t bits_mix(uint64_t x) {
    uint64_t r = __builtin_clzll(x | 1) + (__builtin_ctzll(x | 0x8000000000000000ull) << 8);
    r ^= __builtin_bswap64(x);
    r += __builtin_bitreverse32((uint32_t)x);
    r += (x >> 13) | (x << 51);
    r += (uint64_t)(int64_t)(int8_t)(x >> 3);
    r += (uint64_t)(int64_t)(int16_t)(x >> 7);
    r += (uint64_t)(int64_t)(int32_t)(x >> 1);
    return r;
}

struct packed { unsigned a : 3, b : 7, c : 11, d : 11; };
uint32_t bitfields(uint32_t seed) {
    struct packed p = {0};
    p.a = seed; p.b = seed >> 3; p.c = seed * 7; p.d = p.a + p.b + p.c;
    return p.a * 1000000u + p.b * 10000u + p.c * 10u + p.d;
}

double fp_poly(double x) { return 3.0 * x * x - 2.5 * x + 1.25; }
float fp_mix(float a, float b) { return a > b ? a / b + 0.5f : b * a - 1.0f; }
int64_t fp_to_int(double x) { return (int64_t)x + (int64_t)(uint32_t)(x * 2.0) + (int64_t)(float)x; }
double int_to_fp(int64_t a, uint32_t b) { return (double)a / 3.0 + (double)b * 0.25 + (float)a; }
double fp_sqrt_abs(double x) { return __builtin_sqrt(x < 0 ? -x : x); }

uint32_t fnv1a(const char *s) {
    uint32_t h = 2166136261u;
    while (*s) { h ^= (uint8_t)*s++; h *= 16777619u; }
    return h;
}
uint32_t hash_local_string(uint32_t n) {
    char buf[32];
    const char *src = "pomme runs arm64 code";
    size_t i = 0;
    for (; src[i] && i < 31; i++) buf[i] = src[i] + (char)(n % 3);
    buf[i] = 0;
    return fnv1a(buf);
}

uint32_t ackermann(uint32_t m, uint32_t n) {
    if (m == 0) return n + 1;
    if (n == 0) return ackermann(m - 1, 1);
    return ackermann(m - 1, ackermann(m, n - 1));
}

int64_t insertion_sort_checksum(uint32_t seed) {
    int32_t a[24];
    for (int i = 0; i < 24; i++) { seed = seed * 1103515245u + 12345u; a[i] = (int32_t)(seed >> 8) - (1 << 22); }
    for (int i = 1; i < 24; i++) {
        int32_t k = a[i]; int j = i - 1;
        while (j >= 0 && a[j] > k) { a[j + 1] = a[j]; j--; }
        a[j + 1] = k;
    }
    int64_t s = 0;
    for (int i = 0; i < 24; i++) s = s * 31 + a[i];
    return s;
}

struct big { uint64_t w[6]; };
uint64_t struct_copy(uint64_t x) {
    struct big a = {{x, x + 1, x * 3, x ^ 0xff, x << 4, ~x}};
    struct big b = a;
    b.w[2] += a.w[5];
    return b.w[0] + b.w[1] + b.w[2] + b.w[3] + b.w[4] + b.w[5];
}

static uint64_t counter;
uint64_t atomics(uint64_t n) {
    for (uint64_t i = 0; i < n; i++) __atomic_fetch_add(&counter, i, __ATOMIC_SEQ_CST);
    uint64_t expected = 0;
    __atomic_compare_exchange_n(&counter, &expected, 1, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
    uint64_t v = __atomic_exchange_n(&counter, 0, __ATOMIC_ACQ_REL);
    return v;
}

int64_t many_args(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e, int64_t f,
                  int64_t g, int64_t h, int64_t i, int64_t j) {
    return a - b + c * d - e + f * g - h + i * j;
}

// Calls into a system library: should trap on the HLE stub for _puts.
int calls_puts(void) { return puts("hello from the guest") + 1; }

// React Native's RCTGetCornerInsets: pairs of doubles, fmax against zero.
// Clang vectorizes it into FSUB.2D / INS / MOVI / FMAXNM.2D.
#include <math.h>
typedef struct { double tlh, tlv, trh, trv, blh, blv, brh, brv; } radii;
typedef struct { double w, h; } size2;
typedef struct { size2 tl, tr, bl, br; } corners;
typedef struct { double top, left, bottom, right; } insets;
__attribute__((noinline)) corners corner_insets(radii r, insets e) {
    return (corners){{fmax(0, r.tlh - e.left), fmax(0, r.tlv - e.top)},
                     {fmax(0, r.trh - e.right), fmax(0, r.trv - e.top)},
                     {fmax(0, r.blh - e.left), fmax(0, r.blv - e.bottom)},
                     {fmax(0, r.brh - e.right), fmax(0, r.brv - e.bottom)}};
}
double corner_insets_sum(double radius, double border) {
    radii r = {radius, radius + 1, radius + 2, radius + 3, radius + 4, radius + 5, radius + 6, radius + 7};
    insets e = {border, border * 2, border * 3, border * 4};
    corners c = corner_insets(r, e);
    // Weight every field so a swapped or zeroed lane shows.
    return c.tl.w + 2 * c.tl.h + 4 * c.tr.w + 8 * c.tr.h + 16 * c.bl.w + 32 * c.bl.h + 64 * c.br.w + 128 * c.br.h;
}

// React Native Fabric's ensureNoOverlap: float radii scaled by edge sums,
// against a double size (vectorizes into FCVTN/FCMGT-zero/BSL/FMIN on floats).
typedef struct { float v, h; } cradius;
typedef struct { cradius tl, tr, bl, br; } bradii;
static inline float fminf_(float a, float b) { return a < b ? a : b; }
__attribute__((noinline)) bradii no_overlap(bradii r, double width, double height) {
    float left = r.tl.v + r.bl.v, top = r.tl.h + r.tr.h, right = r.tr.v + r.br.v, bottom = r.bl.h + r.br.h;
    float ls = left > 0 ? fminf_(height / left, 1) : 0;
    float ts = top > 0 ? fminf_(width / top, 1) : 0;
    float rs = right > 0 ? fminf_(height / right, 1) : 0;
    float bs = bottom > 0 ? fminf_(width / bottom, 1) : 0;
    return (bradii){{r.tl.v * fminf_(ts, ls), r.tl.h * fminf_(ts, ls)},
                    {r.tr.v * fminf_(ts, rs), r.tr.h * fminf_(ts, rs)},
                    {r.bl.v * fminf_(bs, ls), r.bl.h * fminf_(bs, ls)},
                    {r.br.v * fminf_(bs, rs), r.br.h * fminf_(bs, rs)}};
}
double no_overlap_sum(double radius, double width, double height) {
    float x = (float)radius;
    bradii r = {{x, x}, {x, x}, {x, x}, {x, x}};
    bradii o = no_overlap(r, width, height);
    return o.tl.v + 2 * o.tl.h + 4 * o.tr.v + 8 * o.tr.h + 16 * o.bl.v + 32 * o.bl.h + 64 * o.br.v + 128 * o.br.h;
}

// Float pairs widened to doubles and swapped (React Native's
// RCTCornerRadiiFromBorderRadii): FCVTL + EXT.
typedef struct { float v, h; } fpair;
typedef struct { double h, v; } dpair;
__attribute__((noinline)) void widen_pairs(const fpair *in, dpair *out, int n) {
    for (int i = 0; i < n; i++) out[i] = (dpair){in[i].h, in[i].v};
}
double widen_sum(double a, double b) {
    fpair in[4] = {{a, b}, {b, a}, {a + 1, b + 1}, {b - 1, a - 1}};
    dpair out[4];
    widen_pairs(in, out, 4);
    double s = 0;
    for (int i = 0; i < 4; i++) s = s * 3 + out[i].h * 5 + out[i].v;
    return s;
}

// SHLL / SHLL2 (shift left long by the element size), as Skia's SkRecordCanvas
// uses to pack rounded bounds: each lane widened, then moved to the top half.
#include <arm_neon.h>
uint64_t shll_mix(uint32_t a, uint32_t b, uint16_t c) {
    int32x4_t v = {(int32_t)a, (int32_t)b, (int32_t)(a ^ b), -7};
    int64x2_t lo = vshll_n_s32(vget_low_s32(v), 32);
    int64x2_t hi = vshll_high_n_s32(v, 32);
    uint16x8_t w = vdupq_n_u16(c);
    w = vsetq_lane_u16((uint16_t)(c + 3), w, 5);
    uint32x4_t wl = vshll_n_u16(vget_low_u16(w), 16);
    uint32x4_t wh = vshll_high_n_u16(w, 16);
    uint64_t s = 0;
    s = s * 31 + (uint64_t)vgetq_lane_s64(lo, 0) + (uint64_t)vgetq_lane_s64(lo, 1);
    s = s * 31 + (uint64_t)vgetq_lane_s64(hi, 0) + (uint64_t)vgetq_lane_s64(hi, 1);
    s = s * 31 + vgetq_lane_u32(wl, 0) + vgetq_lane_u32(wh, 1);
    return s;
}

// Scalar FMUL / FMLA by element (Skia's SkRRect::setRectXY): fmul s, s, v.s[i].
float scale_by_lane(float a, float b, float c) {
    float32x4_t v = {a, b, c, a + b};
    float x = vmuls_laneq_f32(c, v, 1);   // c * b
    x = vfmas_laneq_f32(x, a, v, 3);      // x + a * (a + b)
    float64x2_t halves = {0.5, 4.0};
    double y = vmuld_laneq_f64((double)x, halves, 1);
    return (float)y;
}

// FCVTN / FCVTL with half precision (Impeller packs vertex data as halves).
uint64_t to_halves(float a, float b, float c, float d) {
    float32x4_t v = {a, b, c, d};
    float16x4_t h = vcvt_f16_f32(v);
    return vget_lane_u64(vreinterpret_u64_f16(h), 0);
}

float from_halves(uint64_t bits) {
    float16x4_t h = vreinterpret_f16_u64(vcreate_u64(bits));
    float32x4_t v = vcvt_f32_f16(h);
    return vgetq_lane_f32(v, 0) + 2 * vgetq_lane_f32(v, 1) + 4 * vgetq_lane_f32(v, 2) + 8 * vgetq_lane_f32(v, 3);
}

// FCVTMS / FCVTPS / FCVTNS (vector): floor, ceil and nearest-even to integers.
int64_t round_lanes(float a, float b) {
    float32x2_t v = {a, b};
    int32x2_t m = vcvtm_s32_f32(v), p = vcvtp_s32_f32(v), n = vcvtn_s32_f32(v);
    return (int64_t)vget_lane_s32(m, 0) * 1000000 + vget_lane_s32(m, 1) * 10000 + vget_lane_s32(p, 0) * 100 + vget_lane_s32(n, 1);
}
