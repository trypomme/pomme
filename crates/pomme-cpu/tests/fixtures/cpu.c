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

// SDOT / UDOT from the dot-product extension (iPhone A12 and later; SQLite's
// journal checksum vectorizes to UDOT): vector, 64-bit and by-element forms.
__attribute__((target("dotprod")))
uint64_t dot_products(uint64_t seed) {
    uint8_t a[16], b[16];
    for (int i = 0; i < 16; i++) {
        seed = seed * 6364136223846793005ULL + 1442695040888963407ULL;
        a[i] = (uint8_t)(seed >> 56);
        b[i] = (uint8_t)(seed >> 48);
    }
    uint8x16_t ua = vld1q_u8(a), ub = vld1q_u8(b);
    int8x16_t sa = vreinterpretq_s8_u8(ua), sb = vreinterpretq_s8_u8(ub);
    uint32x4_t u = vdotq_u32(vdupq_n_u32(7), ua, ub);
    int32x4_t s = vdotq_s32(vdupq_n_s32(-3), sa, sb);
    int32x4_t l = vdotq_laneq_s32(vdupq_n_s32(0), sa, sb, 2);
    uint32x2_t h = vdot_u32(vdup_n_u32(1), vget_low_u8(ua), vget_high_u8(ub));
    return (uint64_t)vaddvq_u32(u) * 3 + (uint64_t)(uint32_t)vaddvq_s32(s) * 5 + (uint64_t)(uint32_t)vaddvq_s32(l) * 7 + (uint64_t)vaddv_u32(h);
}

// CRC32 / CRC32C (zlib's crc32 uses them): every width, both polynomials.
#include <arm_acle.h>
__attribute__((target("crc")))
uint64_t crc_mix(uint64_t x) {
    uint32_t a = __crc32b(0xffffffff, (uint8_t)x);
    a = __crc32h(a, (uint16_t)(x >> 8));
    a = __crc32w(a, (uint32_t)(x >> 16));
    a = __crc32d(a, x);
    uint32_t c = __crc32cb(0, (uint8_t)x);
    c = __crc32ch(c, (uint16_t)x);
    c = __crc32cw(c, (uint32_t)x);
    c = __crc32cd(c, ~x);
    return ((uint64_t)a << 32) | c;
}

// The cryptographic extension (BoringSSL's TLS, Abseil's Randen). AES-128 on
// FIPS-197's example, encrypting with AESE/AESMC and decrypting with
// AESD/AESIMC; `sel` picks a half of the ciphertext (0, 1) or plaintext (2, 3).
__attribute__((target("aes")))
uint64_t aes128(int sel) {
    uint8_t key[16], text[16];
    for (int i = 0; i < 16; i++) {
        key[i] = (uint8_t)i;
        text[i] = (uint8_t)(i * 0x11);
    }
    static const uint8_t rcon[10] = {1, 2, 4, 8, 16, 32, 64, 128, 0x1b, 0x36};
    uint32_t w[44];
    for (int i = 0; i < 4; i++) w[i] = key[4 * i] | key[4 * i + 1] << 8 | key[4 * i + 2] << 16 | (uint32_t)key[4 * i + 3] << 24;
    for (int i = 4; i < 44; i++) {
        uint32_t t = w[i - 1];
        if (i % 4 == 0) {
            // SubWord with AESE on equal columns, where ShiftRows does nothing.
            uint8x16_t s = vaeseq_u8(vreinterpretq_u8_u32(vdupq_n_u32(t)), vdupq_n_u8(0));
            t = vgetq_lane_u32(vreinterpretq_u32_u8(s), 0);
            t = ((t >> 8) | (t << 24)) ^ rcon[i / 4 - 1];
        }
        w[i] = w[i - 4] ^ t;
    }
    uint8x16_t k[11], dk[11];
    for (int r = 0; r < 11; r++) k[r] = vld1q_u8((const uint8_t *)&w[4 * r]);
    uint8x16_t s = vld1q_u8(text);
    for (int r = 0; r < 9; r++) s = vaesmcq_u8(vaeseq_u8(s, k[r]));
    s = veorq_u8(vaeseq_u8(s, k[9]), k[10]);
    uint8x16_t c = s;
    dk[0] = k[10];
    for (int r = 1; r < 10; r++) dk[r] = vaesimcq_u8(k[10 - r]);
    dk[10] = k[0];
    for (int r = 0; r < 9; r++) s = vaesimcq_u8(vaesdq_u8(s, dk[r]));
    s = veorq_u8(vaesdq_u8(s, dk[9]), dk[10]);
    uint64x2_t out = vreinterpretq_u64_u8(sel < 2 ? c : s);
    return (sel & 1) ? vgetq_lane_u64(out, 1) : vgetq_lane_u64(out, 0);
}

// "abc" as SHA's one padded block of big-endian words.
static void abc_block(uint32_t m[16]) {
    for (int i = 0; i < 16; i++) m[i] = 0;
    m[0] = 0x61626380;
    m[15] = 24;
}

// SHA-256 of "abc" with SHA256H/H2/SU0/SU1; `sel` picks a pair of digest words.
__attribute__((target("sha2")))
uint64_t sha256_abc(int sel) {
    static const uint32_t K[64] = {
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2};
    static const uint32_t H[8] = {0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19};
    uint32_t m[16];
    abc_block(m);
    uint32x4_t msg[4];
    for (int i = 0; i < 4; i++) msg[i] = vld1q_u32(&m[4 * i]);
    uint32x4_t s0 = vld1q_u32(H), s1 = vld1q_u32(H + 4);
    for (int i = 0; i < 16; i++) {
        uint32x4_t t = vaddq_u32(msg[i % 4], vld1q_u32(&K[4 * i]));
        if (i < 12) msg[i % 4] = vsha256su1q_u32(vsha256su0q_u32(msg[i % 4], msg[(i + 1) % 4]), msg[(i + 2) % 4], msg[(i + 3) % 4]);
        uint32x4_t prev = s0;
        s0 = vsha256hq_u32(s0, s1, t);
        s1 = vsha256h2q_u32(s1, prev, t);
    }
    uint32_t d[8];
    vst1q_u32(d, vaddq_u32(s0, vld1q_u32(H)));
    vst1q_u32(d + 4, vaddq_u32(s1, vld1q_u32(H + 4)));
    return ((uint64_t)d[2 * sel] << 32) | d[2 * sel + 1];
}

// SHA-1 of "abc" with SHA1C/P/M/H/SU0/SU1; `sel` picks a pair of digest words.
__attribute__((target("sha2")))
uint64_t sha1_abc(int sel) {
    static const uint32_t H[5] = {0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0};
    static const uint32_t K[4] = {0x5a827999, 0x6ed9eba1, 0x8f1bbcdc, 0xca62c1d6};
    uint32_t m[16];
    abc_block(m);
    uint32x4_t msg[4];
    for (int i = 0; i < 4; i++) msg[i] = vld1q_u32(&m[4 * i]);
    uint32x4_t abcd = vld1q_u32(H);
    uint32_t e = H[4];
    for (int i = 0; i < 20; i++) {
        uint32x4_t t = vaddq_u32(msg[i % 4], vdupq_n_u32(K[i / 5]));
        if (i < 16) msg[i % 4] = vsha1su1q_u32(vsha1su0q_u32(msg[i % 4], msg[(i + 1) % 4], msg[(i + 2) % 4]), msg[(i + 3) % 4]);
        uint32_t next = vsha1h_u32(vgetq_lane_u32(abcd, 0));
        if (i < 5) abcd = vsha1cq_u32(abcd, e, t);
        else if (i < 10 || i >= 15) abcd = vsha1pq_u32(abcd, e, t);
        else abcd = vsha1mq_u32(abcd, e, t);
        e = next;
    }
    uint32_t d[6];
    d[5] = 0;
    vst1q_u32(d, vaddq_u32(abcd, vld1q_u32(H)));
    d[4] = e + H[4];
    return ((uint64_t)d[2 * sel] << 32) | d[2 * sel + 1];
}

// PMULL / PMULL2 on 64-bit halves and on bytes, folded into one value.
__attribute__((target("aes")))
uint64_t poly_mul(uint64_t a, uint64_t b) {
    poly128_t lo = vmull_p64(a, b);
    poly64x2_t va = vcombine_p64(vcreate_p64(b), vcreate_p64(a)), vb = vcombine_p64(vcreate_p64(a), vcreate_p64(~b));
    poly128_t hi = vmull_high_p64(va, vb);
    poly16x8_t bytes = vmull_p8(vcreate_p8(a), vcreate_p8(b));
    uint64x2_t by = vreinterpretq_u64_p16(bytes);
    uint64_t l0 = (uint64_t)lo, l1 = (uint64_t)(lo >> 64), h0 = (uint64_t)hi, h1 = (uint64_t)(hi >> 64);
    return l0 ^ (l1 * 3) ^ (h0 * 5) ^ (h1 * 7) ^ (vgetq_lane_u64(by, 0) * 11) ^ (vgetq_lane_u64(by, 1) * 13);
}
