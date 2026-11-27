//! Metal shader wallpapers.
//!
//! Same window plumbing as the video path, but instead of an `AVPlayerLayer`
//! each screen gets a `CAMetalLayer` that a background thread draws a fragment
//! shader into every frame.  The shader sees the ShaderToy-style uniforms
//! `iTime` / `iResolution` / `iFrame` (named `time`, `resolution`, `frame`
//! here), so procedural backgrounds need no video file at all.

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{msg_send, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSApplication, NSApplicationActivationPolicy, NSBackingStoreType,
    NSColor, NSScreen, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSDate, NSRect, NSRunLoop, NSString};
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLDrawable, MTLLibrary,
    MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRenderPassDescriptor, MTLRenderCommandEncoder,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLStorageMode, MTLStoreAction, MTLTexture,
    MTLTextureUsage, MTLCreateSystemDefaultDevice,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use crate::SHOULD_QUIT;

const WINDOW_LEVEL: isize = -10000;

/// Prepended to every user shader.  The user file only has to define
/// `fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]])`.
const PREAMBLE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct Uniforms {
    float2 resolution;
    float  time;
    float  frame;
    float2 mouse;
};

struct VOut {
    float4 position [[position]];
    float2 uv;
};

vertex VOut v_main(uint vid [[vertex_id]]) {
    float2 p[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    VOut o;
    o.position = float4(p[vid], 0.0, 1.0);
    o.uv = p[vid] * 0.5 + 0.5;
    return o;
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    resolution: [f32; 2],
    time: f32,
    frame: f32,
    mouse: [f32; 2],
}

unsafe impl Send for Gpu {}
unsafe impl Sync for Gpu {}

struct Gpu {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}

// ---- built-ins ------------------------------------------------------------

pub const BUILTINS: &[(&str, &str)] = &[
    (
        "plasma",
        r#"
fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 p = in.uv * 2.0 - 1.0;
    p.x *= u.resolution.x / max(u.resolution.y, 1.0);
    float t = u.time * 0.25;
    float v = sin(p.x * 3.0 + t)
            + sin(p.y * 4.0 - t * 1.3)
            + sin((p.x + p.y) * 2.5 + t * 0.7)
            + sin(length(p) * 6.0 - t * 2.0);
    v *= 0.25;
    float3 col = mix(float3(0.035, 0.045, 0.040), float3(0.09, 0.52, 0.29),
                     smoothstep(-0.55, 0.85, v));
    col += float3(0.015, 0.035, 0.022) * sin(v * 6.2831);
    return float4(col, 1.0);
}
"#,
    ),
    (
        "aurora",
        r#"
static float hash(float2 p) {
    return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
}
static float noise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    float a = hash(i);
    float b = hash(i + float2(1.0, 0.0));
    float c = hash(i + float2(0.0, 1.0));
    float d = hash(i + float2(1.0, 1.0));
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 uv = in.uv;
    float aspect = u.resolution.x / max(u.resolution.y, 1.0);
    float2 p = float2(uv.x * aspect, uv.y);
    float t = u.time * 0.06;

    float3 col = mix(float3(0.010, 0.016, 0.030), float3(0.020, 0.040, 0.070), uv.y);

    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float band = 0.62 - fi * 0.13;
        float wobble = noise(float2(p.x * 2.2 + fi * 4.0, t * 2.0 + fi)) * 0.22;
        float d = abs(p.y - (band + wobble + 0.06 * sin(p.x * 3.1 + t * 6.0 + fi)));
        float glow = exp(-d * 26.0);
        float3 tint = mix(float3(0.05, 0.45, 0.32), float3(0.10, 0.25, 0.55), fi / 2.0);
        col += tint * glow * (0.55 - fi * 0.12);
    }

    col += float3(0.010, 0.020, 0.015) * noise(p * 60.0 + t * 10.0);
    return float4(col, 1.0);
}
"#,
    ),
    (
        "rings",
        r#"
fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 p = in.uv * 2.0 - 1.0;
    p.x *= u.resolution.x / max(u.resolution.y, 1.0);
    float t = u.time * 0.35;
    float r = length(p);

    float rings = sin(r * 22.0 - t * 3.0) * 0.5 + 0.5;
    rings = pow(rings, 3.0);
    float core = exp(-r * 2.4);
    float3 col = float3(0.030, 0.034, 0.032);
    col += float3(0.06, 0.42, 0.24) * rings * (1.0 - smoothstep(0.2, 1.1, r));
    col += float3(0.10, 0.30, 0.20) * core * 0.8;
    col *= 1.0 - smoothstep(0.9, 1.5, r) * 0.7;
    return float4(col, 1.0);
}
"#,
    ),
    (
        "stark",
        r#"
// Stark — a workshop hologram. A projected light-grid floor runs to a glowing
// horizon; above it a gyroscope of tilted rings turns around segmented,
// counter-rotating bands and a hexagonal core, with satellites riding the
// rings. Two data panels hang either side (bar columns, scrolling readouts, a
// live waveform, a gauge), motes of light drift up through the projection, and
// every so often the signal tears and fringes like a real projector would.
static float hash21(float2 p) {
    p = fract(p * float2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}
static float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + float2(1, 0)), w.x),
               mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), w.x), w.y);
}
static float2 rot(float2 p, float a) {
    float c = cos(a), s = sin(a);
    return float2(c * p.x - s * p.y, s * p.x + c * p.y);
}
static float hair(float d, float w) { return 1.0 - smoothstep(0.0, w, d); }
static float sdBox(float2 p, float2 b) {
    float2 d = abs(p) - b;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}
static float sdChamfer(float2 p, float2 b, float c) {
    return max(sdBox(p, b), (abs(p.x) + abs(p.y) - (b.x + b.y - c)) * 0.7071);
}
static float sdHex(float2 p, float r) {
    const float3 k = float3(-0.866025404, 0.5, 0.577350269);
    p = abs(p);
    p -= 2.0 * min(dot(k.xy, p), 0.0) * k.xy;
    p -= float2(clamp(p.x, -k.z * r, k.z * r), r);
    return length(p) * sign(p.y);
}

// A panel frame: chamfered outline, a header rule, and brackets on two corners.
static float panelFrame(float2 q, float2 b, float px) {
    float f = hair(abs(sdChamfer(q, b, 0.05)), px * 1.4) * 0.55;
    f += hair(abs(q.y - (b.y - 0.07)), px) * step(abs(q.x), b.x - 0.03) * 0.35;
    float2 cq = abs(q) - b - 0.025;
    float br = hair(abs(cq.y), px * 1.6) * step(abs(cq.x + 0.04), 0.04)
             + hair(abs(cq.x), px * 1.6) * step(abs(cq.y + 0.04), 0.04);
    return f + br * 0.9;
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float aspect = res.x / max(res.y, 1.0);
    float t = u.time;
    float2 uv = in.uv;
    float px = 2.0 / max(res.y, 1.0);

    // ---- signal tears: a few rows slip sideways for a moment, every ~10 s
    float slot = floor(t * 1.4);
    float tearing = step(0.94, hash21(float2(slot, 7.1)));
    float bandY = hash21(float2(slot, 3.3));
    float inBand = step(abs(uv.y - bandY), 0.03 + 0.05 * hash21(float2(slot, 9.2)));
    float tear = tearing * inBand;
    uv.x += tear * (hash21(float2(slot, floor(uv.y * 60.0))) - 0.5) * 0.06;

    float2 p = (uv - 0.5) * float2(aspect, 1.0) * 2.0;

    float3 cyan  = float3(0.24, 0.84, 1.00);
    float3 ice   = float3(0.86, 0.97, 1.00);
    float3 amber = float3(1.00, 0.34, 0.22);

    // ---- the room: deep blue-black, a slow haze lit by the projection
    float3 col = mix(float3(0.004, 0.008, 0.016), float3(0.010, 0.026, 0.046), smoothstep(-1.0, 0.8, p.y));
    float haze = vnoise(p * 1.3 + float2(t * 0.025, t * 0.01)) * 0.6 + vnoise(p * 3.1 - t * 0.04) * 0.4;
    col += cyan * 0.030 * haze * exp(-length(p - float2(0.0, 0.1)) * 0.9);

    // ---- the light-grid floor, rolling toward you
    float horizon = -0.30;
    float below = horizon - p.y;
    if (below > 0.0) {
        float z = 0.32 / below;
        float2 g = float2(p.x * z * 1.2, z + t * 0.45);
        float2 fw = fwidth(g) * 1.4;
        float2 f = abs(fract(g) - 0.5);
        float lines = max(hair(0.5 - f.x, fw.x), hair(0.5 - f.y, fw.y));
        float fade = exp(-z * 0.22) * smoothstep(0.0, 0.04, below);
        col += cyan * lines * fade * 0.42;
        // the projection's footprint on the floor
        col += cyan * 0.06 * exp(-length(float2(p.x * 0.8, (p.y - horizon) * 3.0)) * 2.0);
    }
    col += cyan * exp(-abs(p.y - horizon) * 70.0) * 0.22 * exp(-abs(p.x) * 0.6);

    // ---- the projection
    float sc = 0.80;
    float2 c = float2(0.0, 0.14);
    float2 q = p - c;
    float r = length(q);
    float a = atan2(q.y, q.x);
    float boot = smoothstep(0.0, 2.5, t);   // rings draw in when it starts

    // segmented bands, alternating direction
    for (int i = 0; i < 5; i++) {
        float fi = float(i);
        float R = sc * (0.20 + fi * 0.085);
        float N = (i % 2 == 0) ? (36.0 + fi * 12.0) : (6.0 + fi * 3.0);
        float speed = ((i % 2 == 0) ? 1.0 : -1.0) * (0.04 + 0.02 * fi);
        float duty = (i % 2 == 0) ? 0.55 : 0.72;
        float seg = step(fract(a / 6.2831853 * N + t * speed), duty);
        seg *= step(fract(a / 6.2831853 + 0.5), boot);
        float d = abs(r - R);
        col += cyan * seg * (hair(d, px * (1.4 + fi * 0.25)) * 0.85 + exp(-d * 70.0) * 0.20);
        // the gaps keep a faint trace of the band, so they read as dashes on a
        // track rather than holes
        col += cyan * (hair(d, px * 1.2) * 0.10 + exp(-d * 70.0) * 0.06);
    }

    // the main ring, with a projector's colour fringe
    {
        float R = sc * 0.66;
        float w = px * 2.2;
        float3 ring = float3(hair(abs(r * 1.004 - R), w), hair(abs(r - R), w), hair(abs(r * 0.996 - R), w));
        col += ring * cyan * 1.1 + cyan * exp(-abs(r - R) * 28.0) * 0.30;
        // an amber arc that blinks, like something needs attention
        float sweep = fract(t * 0.03);
        float da = abs(fract(a / 6.2831853 - sweep + 0.5) - 0.5);
        float blink = 0.55 + 0.45 * step(0.5, fract(t * 1.6));
        col += amber * hair(abs(r - R * 1.035), px * 2.5) * step(da, 0.035) * blink;
    }

    // tick ring: fine ticks, every sixth one longer
    {
        float N = 120.0;
        float ta = fract(a / 6.2831853 * N - t * 0.01);
        float arc = abs(ta - 0.5) / N * 6.2831853 * r;
        float major = step(fract((floor(a / 6.2831853 * N - t * 0.01)) / 6.0), 0.01);
        float R0 = sc * 0.74;
        float len = major > 0.5 ? 0.045 : 0.022;
        float inRing = step(R0, r) * step(r, R0 + len * sc);
        col += cyan * hair(abs(arc - 0.5 / N * 6.2831853 * r), px * 1.2) * inRing * (major > 0.5 ? 0.8 : 0.45);
    }

    // the gyroscope: tilted rings turning about the core, satellites riding them
    for (int k = 0; k < 3; k++) {
        float fk = float(k);
        float phi = t * (0.07 + 0.03 * fk) * ((k == 1) ? -1.0 : 1.0) + fk * 2.1;
        float R = sc * (0.90 + 0.07 * fk);
        float B = R * (0.22 + 0.16 * fk + 0.06 * sin(t * 0.2 + fk));
        float2 e = rot(q, -phi);
        float d = abs((length(e / float2(R, B)) - 1.0) * min(R, B));
        float front = 0.45 + 0.55 * smoothstep(B * 0.3, -B * 0.3, e.y);
        col += cyan * front * (hair(d, px * 1.5) * 0.75 + exp(-d * 90.0) * 0.20);
        float sa = t * (0.45 + 0.15 * fk) + fk * 1.7;
        float2 sp = rot(float2(R * cos(sa), B * sin(sa)), phi);
        float sd = length(q - sp);
        col += ice * exp(-sd * 140.0) * 1.3 + cyan * exp(-sd * 22.0) * 0.25;
    }

    // radar sweep inside the bands
    {
        float sw = t * 0.5;
        float da = fract((a - sw) / 6.2831853);
        col += cyan * pow(1.0 - da, 6.0) * smoothstep(sc * 0.62, sc * 0.16, r) * 0.12;
    }

    // the core: two hexagons turning against each other, and a heart that breathes
    {
        float h1 = sdHex(rot(q, t * 0.15), sc * 0.12);
        float h2 = sdHex(rot(q, -t * 0.32 + 0.52), sc * 0.072);
        col += cyan * (hair(abs(h1), px * 1.8) * 0.95 + exp(-abs(h1) * 45.0) * 0.3);
        col += cyan * hair(abs(h2), px * 1.4) * 0.7;
        float pulse = 0.78 + 0.22 * sin(t * 2.1);
        col += ice * exp(-r * 30.0 / sc) * 1.25 * pulse;
        col += cyan * exp(-r * 6.5 / sc) * 0.28 * pulse;
    }

    // crosshair through the projection
    float cross = exp(-abs(q.y) * 260.0) + exp(-abs(q.x) * 260.0);
    col += cyan * cross * smoothstep(sc * 1.05, sc * 0.30, r) * smoothstep(sc * 0.13, sc * 0.2, r) * 0.30;

    // ---- the left panel: bar columns and a scrolling readout
    {
        float2 L = float2(-aspect + 0.58, 0.16);
        float2 b = float2(0.36, 0.46);
        float2 lq = p - L;
        if (sdBox(lq, b + 0.06) < 0.0) {
            col += cyan * panelFrame(lq, b, px);
            col += cyan * 0.025 * step(sdChamfer(lq, b, 0.05), 0.0);
            // columns
            float cols = 14.0;
            float x = (lq.x + b.x - 0.04) / (2.0 * b.x - 0.08) * cols;
            float ci = floor(x);
            if (ci >= 0.0 && ci < cols && fract(x) < 0.62) {
                float hgt = 0.35 + 0.65 * (0.5 + 0.5 * sin(t * (0.6 + hash21(float2(ci, 1.0))) + ci * 1.3));
                float top = -b.y + 0.05 + hgt * 0.38;
                float on = step(-b.y + 0.05, lq.y) * step(lq.y, top);
                col += cyan * on * mix(0.18, 0.55, smoothstep(-b.y, top, lq.y));
                col += ice * hair(abs(lq.y - top), px * 1.2) * step(-b.y + 0.05, lq.y) * 0.5;
            }
            // readout rows: dashes standing in for text, scrolling up
            float rowH = 0.034;
            float y = (lq.y - 0.02) / rowH + t * 0.6;
            float ri = floor(y);
            if (lq.y > 0.0 && lq.y < b.y - 0.1 && fract(y) > 0.3 && fract(y) < 0.7) {
                float words = lq.x + b.x - 0.04;
                float wi = floor(words / 0.07);
                float lenR = hash21(float2(ri, 5.0)) * 0.6 + 0.3;
                float on = step(fract(words / 0.07), 0.8) * step(words, lenR * (2.0 * b.x - 0.08)) * step(0.0, words);
                float hot = step(0.86, hash21(float2(ri, wi)));
                col += mix(cyan, amber, hot) * on * 0.38;
            }
        }
    }

    // ---- the right panel: a live waveform and a gauge
    {
        float2 Rc = float2(aspect - 0.58, 0.16);
        float2 b = float2(0.36, 0.46);
        float2 rq = p - Rc;
        if (sdBox(rq, b + 0.06) < 0.0) {
            col += cyan * panelFrame(rq, b, px);
            col += cyan * 0.025 * step(sdChamfer(rq, b, 0.05), 0.0);
            // waveform in the top half, over a fine grid
            float2 wq = rq - float2(0.0, 0.17);
            if (abs(wq.x) < b.x - 0.04 && abs(wq.y) < 0.14) {
                float2 gq = abs(fract(wq * 20.0) - 0.5);
                col += cyan * 0.07 * max(hair(0.5 - gq.x, 0.04), hair(0.5 - gq.y, 0.04));
                float x = wq.x;
                float f = 0.07 * sin(x * 14.0 + t * 2.4) * sin(x * 3.3 - t * 0.7)
                        + 0.025 * sin(x * 41.0 - t * 5.0);
                float d = abs(wq.y - f);
                col += cyan * (hair(d, px * 1.8) * 0.9 + exp(-d * 60.0) * 0.25);
            }
            // gauge in the bottom half
            float2 gq = rq - float2(0.0, -0.20);
            float gr = length(gq);
            float ga = atan2(gq.y, gq.x);
            float fill = 0.55 + 0.35 * sin(t * 0.35);
            float span = fract((ga + 3.14159 * 1.25) / 6.2831853);
            float inArc = step(span, 0.75);
            col += cyan * hair(abs(gr - 0.17), px * 1.2) * inArc * 0.4;
            col += mix(cyan, amber, smoothstep(0.82, 0.9, fill))
                 * hair(abs(gr - 0.17), px * 5.0) * step(span, 0.75 * fill) * 0.9;
            col += ice * exp(-gr * 40.0) * 0.4;
        }
    }

    // ---- motes of light drifting up through the projection
    {
        float2 mp = p * 6.0 + float2(0.0, -t * 0.35);
        float2 id = floor(mp);
        float2 fq = fract(mp) - 0.5;
        float h = hash21(id);
        if (h > 0.78) {
            float2 off = float2(hash21(id + 3.1), hash21(id + 7.7)) - 0.5;
            float d = length(fq - off * 0.7);
            float tw = 0.5 + 0.5 * sin(t * (1.0 + h * 3.0) + h * 40.0);
            col += cyan * exp(-d * 30.0) * 0.35 * tw * exp(-length(p - c) * 0.7);
        }
    }

    // ---- the projector: a scan band, scanlines, flicker, fringe on tears
    float scan = exp(-pow((uv.y - fract(-t * 0.06)) * 22.0, 2.0));
    col += cyan * scan * 0.045;
    col *= 0.90 + 0.10 * sin(uv.y * res.y * 1.4);
    col *= 0.95 + 0.05 * sin(t * 7.3) * sin(t * 2.9);
    if (tear > 0.5) {
        col = float3(col.r * 1.6 + 0.02, col.g * 0.85, col.b * 1.25);
        col += cyan * 0.06;
    }
    col += (hash21(uv * res + fract(t)) - 0.5) * 0.012;
    float2 vq = (in.uv - 0.5) * 2.0;
    col *= 1.0 - 0.45 * dot(vq, vq) * 0.5;

    return float4(max(col, 0.0), 1.0);
}
"#,
    ),
    (
        "arcane",
        r#"
// Arcane — a summoning at night. A circle of light lies on the ground in
// perspective: rings of procedural runes turning against each other, a
// seven-point star and a hexagram, all glowing violet. A pillar of light rises
// out of it into a nebula sky, orbs circle the pillar trailing light, and
// sparks spiral up out of the circle and drift off as fireflies.
static float hash21(float2 p) {
    p = fract(p * float2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}
static float vnoise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float2 w = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + float2(1, 0)), w.x),
               mix(hash21(i + float2(0, 1)), hash21(i + float2(1, 1)), w.x), w.y);
}
static float fbm(float2 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) { v += a * vnoise(p); p = p * 2.03 + 17.0; a *= 0.5; }
    return v;
}
static float2 rot(float2 p, float a) {
    float c = cos(a), s = sin(a);
    return float2(c * p.x - s * p.y, s * p.x + c * p.y);
}
static float sdSeg(float2 p, float2 a, float2 b) {
    float2 pa = p - a, ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}
static float hair(float d, float w) { return 1.0 - smoothstep(0.0, w, d); }

// A star polygon {n/k} of radius r: the lines from each vertex to the k-th next.
static float star(float2 p, float r, int n, int k, float turn) {
    float d = 1e3;
    for (int i = 0; i < n; i++) {
        float a0 = turn + 6.2831853 * float(i) / float(n);
        float a1 = turn + 6.2831853 * float(i + k) / float(n);
        d = min(d, sdSeg(p, r * float2(cos(a0), sin(a0)), r * float2(cos(a1), sin(a1))));
    }
    return d;
}

// One rune: a few strokes picked from a 3x3 lattice by the cell's hash.
static float rune(float2 uv, float seed) {
    float d = 1e3;
    for (int s = 0; s < 3; s++) {
        float h = hash21(float2(seed, float(s) * 7.3));
        float h2 = hash21(float2(seed * 1.7, float(s) * 3.1 + 1.0));
        float2 a = floor(float2(h * 3.0, fract(h * 13.0) * 3.0)) / 2.0;
        float2 b = floor(float2(h2 * 3.0, fract(h2 * 11.0) * 3.0)) / 2.0;
        if (length(a - b) > 0.1) d = min(d, sdSeg(uv, a, b));
    }
    return d;
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float aspect = res.x / max(res.y, 1.0);
    float t = u.time;
    float2 uv = in.uv;
    float2 p = (uv - 0.5) * float2(aspect, 1.0) * 2.0;
    float px = 2.0 / max(res.y, 1.0);

    float3 violet = float3(0.71, 0.42, 1.00);
    float3 rose   = float3(1.00, 0.48, 0.85);
    float3 spark  = float3(0.55, 0.95, 1.00);
    float3 white  = float3(1.00, 0.95, 1.00);

    // ---- the night: indigo, a nebula of violet and rose, stars
    float3 col = mix(float3(0.020, 0.008, 0.040), float3(0.050, 0.020, 0.090), smoothstep(-1.0, 1.0, p.y));
    float neb = fbm(p * 0.9 + float2(t * 0.012, -t * 0.006));
    float neb2 = fbm(p * 1.7 - float2(t * 0.02, 0.0) + 5.0);
    col += violet * 0.26 * smoothstep(0.30, 0.85, neb) * smoothstep(-0.6, 0.8, p.y);
    col += rose * 0.15 * smoothstep(0.42, 0.9, neb2) * smoothstep(-0.2, 1.0, p.y);
    {
        float2 sp = p * 38.0;
        float2 id = floor(sp);
        float h = hash21(id);
        if (h > 0.965) {
            float2 off = float2(hash21(id + 1.3), hash21(id + 4.1)) - 0.5;
            float d = length(fract(sp) - 0.5 - off * 0.6);
            float tw = 0.5 + 0.5 * sin(t * (1.5 + h * 5.0) + h * 80.0);
            col += white * exp(-d * 18.0) * 0.55 * tw * smoothstep(-0.5, 0.4, p.y);
        }
    }
    // an aurora ribbon across the top
    {
        float y = 0.72 + 0.08 * sin(p.x * 1.3 + t * 0.15) + 0.05 * sin(p.x * 3.1 - t * 0.22);
        float band = exp(-abs(p.y - y) * 9.0) * (0.6 + 0.4 * vnoise(float2(p.x * 3.0 - t * 0.3, 0.0)));
        col += mix(violet, rose, 0.5 + 0.5 * sin(p.x * 0.8 + t * 0.1)) * band * 0.16;
    }

    // ---- the ground: a faint mist the circle lights up
    float ground = smoothstep(-0.25, -0.6, p.y);
    col += violet * 0.05 * ground * fbm(float2(p.x * 2.0 + t * 0.05, p.y * 6.0));

    // ---- the circle, lying on the ground in perspective
    float2 C = float2(0.0, -0.52);
    float2 q = p - C;
    q.y /= 0.38;
    float r = length(q);
    float a = atan2(q.y, q.x);
    float pulse = 0.82 + 0.18 * sin(t * 1.7);
    float w = px * 2.2 / 0.38;
    float glow = 0.0;

    for (int i = 0; i < 4; i++) {
        float R = 0.38 + float(i) * 0.12 + (i == 3 ? 0.04 : 0.0);
        float d = abs(r - R);
        glow += hair(d, w * (i == 3 ? 1.6 : 1.0)) * 0.9 + exp(-d * 40.0) * 0.25;
    }
    // rune bands between the outer rings, turning opposite ways
    for (int b = 0; b < 2; b++) {
        float R0 = (b == 0) ? 0.62 : 0.74;
        float R1 = R0 + 0.10;
        if (r > R0 && r < R1) {
            float N = (b == 0) ? 40.0 : 52.0;
            float turn = (b == 0 ? 1.0 : -1.0) * t * 0.05;
            float ang = fract((a / 6.2831853) + turn) * N;
            float cell = floor(ang);
            float2 ruv = float2(fract(ang), (r - R0) / (R1 - R0));
            ruv = (ruv - 0.5) * float2(1.25, 1.25) + 0.5;
            float d = rune(ruv, cell + float(b) * 100.0);
            float lit = 0.7 + 0.3 * sin(t * 2.0 + cell * 0.7);
            glow += hair(d, 0.05) * lit * 0.8 + exp(-d * 18.0) * lit * 0.12;
        }
    }
    // a seven-point star and a hexagram, turning slowly
    float st = star(q, 0.60, 7, 3, t * 0.06);
    glow += hair(st, w * 1.2) * 0.8 + exp(-st * 50.0) * 0.18;
    float hx = star(q, 0.36, 6, 2, -t * 0.1);
    glow += hair(hx, w * 1.1) * 0.7 + exp(-hx * 60.0) * 0.15;
    glow += exp(-r * 6.0) * 0.6;
    col += mix(violet, rose, smoothstep(0.2, 0.9, r)) * glow * pulse;

    // ---- the pillar of light rising out of it
    float height = p.y - C.y;
    {
        float base = smoothstep(-0.10, 0.03, height);
        height = max(height, 0.0);
        float width = 0.13 + 0.015 * sin(height * 6.0 - t * 2.0);
        col += violet * exp(-pow(p.x / (width * 3.5), 2.0)) * exp(-height * 0.7) * 0.18 * pulse * base;
        float beam = exp(-pow(p.x / width, 2.0)) * exp(-height * 0.9);
        float core = exp(-pow(p.x / (width * 0.18), 2.0)) * exp(-height * 1.4);
        float shimmer = 0.75 + 0.25 * vnoise(float2(p.x * 20.0, height * 4.0 - t * 3.0));
        col += (violet * beam * 0.55 * shimmer * pulse + white * core * 0.6 * pulse) * base;
    }

    // ---- orbs circling the pillar, trailing light
    for (int o = 0; o < 3; o++) {
        float fo = float(o);
        for (int k = 0; k < 7; k++) {
            float tk = t - float(k) * 0.05;
            float ang = tk * (0.55 + fo * 0.12) + fo * 2.094;
            float depth = sin(ang);
            float2 op = float2(cos(ang) * (0.62 - fo * 0.08), C.y + 0.45 + fo * 0.28 + depth * 0.09);
            float d = length(p - op);
            float size = (k == 0) ? 1.0 : 0.55 * (1.0 - float(k) / 7.0);
            float near = 0.55 + 0.45 * (depth * -0.5 + 0.5);
            float3 tint = (o == 1) ? spark : mix(rose, violet, fo * 0.5);
            col += tint * exp(-d * (k == 0 ? 55.0 : 90.0)) * size * near * (k == 0 ? 1.4 : 0.6);
            if (k == 0) col += tint * exp(-d * 9.0) * 0.12 * near;
        }
    }

    // ---- sparks spiralling up out of the circle, becoming fireflies
    for (int l = 0; l < 3; l++) {
        float fl = float(l);
        float scale = 7.0 + fl * 5.0;
        float speed = 0.18 + fl * 0.08;
        float2 mp = p * scale;
        mp.y -= t * speed * scale;
        mp.x += sin(p.y * 3.0 + t * 0.8 + fl) * 0.6;
        float2 id = floor(mp);
        float h = hash21(id + fl * 31.0);
        float2 f = fract(mp) - 0.5;
        float2 off = float2(hash21(id + 2.7), hash21(id + 9.1)) - 0.5;
        float2 d2 = f - off * 0.7;
        float d = length(d2);
        // most sparks live in a cone above the circle; a few drift everywhere
        float cone = exp(-abs(p.x) / (0.25 + max(p.y - C.y, 0.0) * 0.55)) * smoothstep(C.y - 0.1, C.y + 0.1, p.y);
        float keep = step(1.0 - (0.08 + 0.75 * cone), h);
        float tw = 0.5 + 0.5 * sin(t * (2.0 + h * 6.0) + h * 50.0);
        float3 tint = h > 0.97 ? spark : (h > 0.9 ? rose : violet);
        float bright = keep * tw * (0.55 + 1.1 * cone);
        col += tint * exp(-d * 26.0) * bright * 0.9;
        // the bright ones glint as four-point stars
        if (h > 0.93) {
            float glint = exp(-abs(d2.x) * 90.0) * exp(-abs(d2.y) * 9.0) + exp(-abs(d2.y) * 90.0) * exp(-abs(d2.x) * 9.0);
            col += white * glint * bright * 0.45;
        }
    }

    // ---- light on the ground around the circle, and a vignette
    col += violet * 0.10 * exp(-length(float2(p.x * 0.7, (p.y - C.y) * 2.2)) * 1.6) * pulse;
    float2 vq = (in.uv - 0.5) * 2.0;
    col *= 1.0 - 0.4 * dot(vq, vq) * 0.5;
    col += (hash21(uv * res + fract(t)) - 0.5) * 0.008;
    return float4(max(col, 0.0), 1.0);
}
"#,
    ),
];

pub fn builtin_source(name: &str) -> Option<&'static str> {
    BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

pub fn builtin_names() -> Vec<&'static str> {
    BUILTINS.iter().map(|(n, _)| *n).collect()
}

// ---- pipeline -------------------------------------------------------------

fn build_gpu(source: &str) -> Result<Gpu, String> {
    unsafe {
        let device = MTLCreateSystemDefaultDevice().ok_or("no Metal device on this Mac")?;
        let queue = device.newCommandQueue().ok_or("could not create a command queue")?;

        let full = format!("{PREAMBLE}\n{source}");
        let ns_src = NSString::from_str(&full);
        let library = device
            .newLibraryWithSource_options_error(&ns_src, None)
            .map_err(|e| format!("shader compile failed:\n{}", e.localizedDescription()))?;

        let vname = NSString::from_str("v_main");
        let fname = NSString::from_str("f_main");
        let vfn = library
            .newFunctionWithName(&vname)
            .ok_or("shader is missing the vertex function v_main")?;
        let ffn = library
            .newFunctionWithName(&fname)
            .ok_or("shader must define: fragment float4 f_main(...)")?;

        let desc = MTLRenderPipelineDescriptor::new();
        desc.setVertexFunction(Some(&vfn));
        desc.setFragmentFunction(Some(&ffn));
        {
            let attachments = desc.colorAttachments();
            let a0 = attachments.objectAtIndexedSubscript(0);
            a0.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        }
        let pipeline = device
            .newRenderPipelineStateWithDescriptor_error(&desc)
            .map_err(|e| format!("pipeline build failed:\n{}", e.localizedDescription()))?;

        Ok(Gpu {
            device,
            queue,
            pipeline,
        })
    }
}

unsafe fn encode_frame(
    gpu: &Gpu,
    texture: &ProtocolObject<dyn MTLTexture>,
    uniforms: &Uniforms,
) -> Result<Retained<ProtocolObject<dyn objc2_metal::MTLCommandBuffer>>, String> {
    let pass = MTLRenderPassDescriptor::renderPassDescriptor();
    {
        let attachments = pass.colorAttachments();
        let a0 = attachments.objectAtIndexedSubscript(0);
        a0.setTexture(Some(texture));
        a0.setLoadAction(MTLLoadAction::Clear);
        a0.setStoreAction(MTLStoreAction::Store);
        a0.setClearColor(MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });
    }

    let cb = gpu.queue.commandBuffer().ok_or("no command buffer")?;
    let enc = cb
        .renderCommandEncoderWithDescriptor(&pass)
        .ok_or("no render encoder")?;
    enc.setRenderPipelineState(&gpu.pipeline);
    enc.setFragmentBytes_length_atIndex(
        std::ptr::NonNull::new_unchecked(uniforms as *const Uniforms as *mut c_void),
        std::mem::size_of::<Uniforms>(),
        0,
    );
    enc.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3);
    enc.endEncoding();
    Ok(cb)
}

// ---- offscreen check ------------------------------------------------------

/// Render exactly one frame offscreen and report what came back, so a shader
/// can be verified without looking at the screen.
pub fn check(source: &str, width: u32, height: u32) -> Result<String, String> {
    let (msg, _buf) = check_with_pixels(source, width, height, 1.0)?;
    Ok(msg)
}

/// Same as `check`, plus the raw BGRA pixels so a caller can look at the frame.
pub fn check_with_pixels(
    source: &str,
    width: u32,
    height: u32,
    time: f32,
) -> Result<(String, Vec<u8>), String> {
    let gpu = build_gpu(source)?;
    unsafe {
        let desc = objc2_metal::MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::BGRA8Unorm,
            width as usize,
            height as usize,
            false,
        );
        desc.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        desc.setStorageMode(MTLStorageMode::Shared);
        let texture = gpu
            .device
            .newTextureWithDescriptor(&desc)
            .ok_or("could not allocate the check texture")?;

        let uniforms = Uniforms {
            resolution: [width as f32, height as f32],
            time,
            frame: 1.0,
            mouse: [0.0, 0.0],
        };
        let cb = encode_frame(&gpu, &texture, &uniforms)?;
        cb.commit();
        cb.waitUntilCompleted();

        let row_bytes = width as usize * 4;
        let mut buf = vec![0u8; row_bytes * height as usize];
        let region = objc2_metal::MTLRegion {
            origin: objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
            size: objc2_metal::MTLSize {
                width: width as usize,
                height: height as usize,
                depth: 1,
            },
        };
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            std::ptr::NonNull::new_unchecked(buf.as_mut_ptr() as *mut c_void),
            row_bytes,
            region,
            0,
        );

        let mut sum = [0u64; 3];
        let mut min = [255u8; 3];
        let mut max = [0u8; 3];
        for px in buf.chunks_exact(4) {
            for c in 0..3 {
                let v = px[c];
                sum[c] += v as u64;
                min[c] = min[c].min(v);
                max[c] = max[c].max(v);
            }
        }
        let n = (width * height) as u64;
        let mean = [
            sum[0] / n.max(1),
            sum[1] / n.max(1),
            sum[2] / n.max(1),
        ];
        Ok((format!(
            "ok  {}x{}  mean rgb({},{},{})  min({},{},{})  max({},{},{})",
            width,
            height,
            mean[2],
            mean[1],
            mean[0],
            min[2],
            min[1],
            min[0],
            max[2],
            max[1],
            max[0]
        ), buf))
    }
}

/// Coarse luminance map, for looking at a shader without a screenshot.
pub fn ascii_frame(pixels: &[u8], width: u32, height: u32, cols: u32, rows: u32) -> String {
    const RAMP: &[u8] = b" .:-=+*#%@";
    let mut out = String::new();
    for ry in 0..rows {
        for rx in 0..cols {
            let x0 = rx * width / cols;
            let x1 = ((rx + 1) * width / cols).max(x0 + 1);
            let y0 = ry * height / rows;
            let y1 = ((ry + 1) * height / rows).max(y0 + 1);
            let mut sum = 0u32;
            let mut n = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * width + x) * 4) as usize;
                    let lum = (pixels[i + 2] as u32 * 30
                        + pixels[i + 1] as u32 * 59
                        + pixels[i] as u32 * 11)
                        / 100;
                    sum += lum;
                    n += 1;
                }
            }
            let lum = (sum / n.max(1)) as usize * (RAMP.len() - 1) / 255;
            out.push(RAMP[lum] as char);
        }
        out.push('\n');
    }
    out
}

// ---- live wallpaper -------------------------------------------------------

struct LayerRef(Retained<CAMetalLayer>);
// The layer is created on the main thread and then only touched by its own
// render thread, which is exactly the ownership CAMetalLayer expects.
unsafe impl Send for LayerRef {}

impl LayerRef {
    // A method call, so the closure captures the whole LayerRef (which is Send)
    // instead of its inner field (which is not).
    fn get(&self) -> &CAMetalLayer {
        &self.0
    }
}

struct ScreenWindow {
    window: Retained<NSWindow>,
    layer: LayerRef,
    size: [f64; 2],
}

unsafe fn create_window(mtm: MainThreadMarker, frame: NSRect, gpu: &Gpu) -> ScreenWindow {
    let win: Retained<NSWindow> = msg_send![
        NSWindow::alloc(mtm),
        initWithContentRect: frame,
        styleMask: NSWindowStyleMask::Borderless,
        backing: NSBackingStoreType::Buffered,
        defer: false,
    ];
    win.setTitle(&NSString::from_str("Live Wallpaper"));
    win.setOpaque(true);
    win.setBackgroundColor(Some(&NSColor::blackColor()));
    win.setLevel(WINDOW_LEVEL);
    win.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    win.setIgnoresMouseEvents(true);

    let view: Retained<NSView> = msg_send![NSView::alloc(mtm), initWithFrame: frame];
    view.setWantsLayer(true);
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );

    let layer = CAMetalLayer::new();
    layer.setDevice(Some(&gpu.device));
    layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    layer.setFramebufferOnly(true);
    layer.setOpaque(true);

    // Drawn at one pixel per point and scaled up by the compositor: four times
    // fewer pixels than Retina, which these soft, glowing shaders barely show,
    // and it is what keeps a full-screen shader from saturating the GPU the
    // desktop needs. LWP_SHADER_SCALE=2 for full Retina.
    let scale = std::env::var("LWP_SHADER_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v.clamp(0.25, 2.0))
        .unwrap_or(1.0);
    let drawable = objc2_core_foundation::CGSize {
        width: frame.size.width * scale,
        height: frame.size.height * scale,
    };
    layer.setDrawableSize(drawable);
    let bounds = view.bounds();
    let _: () = msg_send![&layer, setFrame: bounds];

    view.setLayer(Some(&layer));
    win.setContentView(Some(&view));
    win.makeKeyAndOrderFront(None);

    ScreenWindow {
        window: win,
        layer: LayerRef(layer),
        size: [drawable.width, drawable.height],
    }
}

fn spawn_renderer(gpu: Arc<Gpu>, layer: LayerRef, size: [f64; 2]) {
    thread::spawn(move || {
        let layer = layer.get();
        let start = Instant::now();
        let mut frame: u32 = 0;
        let res = [size[0] as f32, size[1] as f32];
        // A wallpaper needs no more than 30 frames a second; drawing at the
        // display's 120 just burns GPU time the windows on top of it want.
        let fps = std::env::var("LWP_SHADER_FPS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v.clamp(5.0, 120.0))
            .unwrap_or(30.0);
        let period = std::time::Duration::from_secs_f64(1.0 / fps);
        let mut next = Instant::now();

        while !SHOULD_QUIT.load(Ordering::SeqCst) {
            let now = Instant::now();
            if now < next {
                thread::sleep(next - now);
            }
            next = (next + period).max(Instant::now());
            unsafe {
                let Some(drawable) = layer.nextDrawable() else {
                    thread::sleep(std::time::Duration::from_millis(8));
                    continue;
                };
                let texture = drawable.texture();
                let uniforms = Uniforms {
                    resolution: res,
                    time: start.elapsed().as_secs_f32(),
                    frame: frame as f32,
                    mouse: [0.0, 0.0],
                };
                match encode_frame(&gpu, &texture, &uniforms) {
                    Ok(cb) => {
                        let _: () = msg_send![&*cb, presentDrawable: &*drawable];
                        cb.commit();
                    }
                    Err(_) => break,
                }
            }
            frame = frame.wrapping_add(1);
        }
    });
}

pub fn run(source: &str, _hide_icons: bool) -> Result<(), String> {
    unsafe {
        libc::signal(libc::SIGTERM, sigterm_handler as *const () as usize);
    }

    let gpu = Arc::new(build_gpu(source)?);

    let mtm = MainThreadMarker::new().ok_or("must run on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();

    let screens = NSScreen::screens(mtm);
    let mut windows: Vec<Arc<ScreenWindow>> = screens
        .iter()
        .map(|s| Arc::new(unsafe { create_window(mtm, s.frame(), &gpu) }))
        .collect();

    for w in &windows {
        spawn_renderer(gpu.clone(), LayerRef(w.layer.0.clone()), w.size);
    }

    let run_loop = NSRunLoop::currentRunLoop();
    while !SHOULD_QUIT.load(Ordering::SeqCst) {
        let limit = NSDate::dateWithTimeIntervalSinceNow(0.1);
        run_loop.runUntilDate(&limit);
    }

    for w in windows.drain(..) {
        unsafe {
            w.window.setReleasedWhenClosed(false);
            w.window.orderOut(None);
            w.window.close();
        }
    }
    Ok(())
}

extern "C" fn sigterm_handler(_: i32) {
    SHOULD_QUIT.store(true, Ordering::SeqCst);
}

#[allow(dead_code)]
fn _assert_send_sync() {
    fn assert_send<T: Send>() {}
    assert_send::<Gpu>();
}

/// Write BGRA pixels (top row first) as a 32-bit BMP — enough for `sips` to turn
/// into a PNG, with no image crate.
pub fn write_bmp(path: &std::path::Path, pixels: &[u8], width: u32, height: u32) -> std::io::Result<()> {
    let row = width as usize * 4;
    let size = 54 + row * height as usize;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    // negative height: rows are stored top-down
    out.extend_from_slice(&(-(height as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in 0..height as usize {
        let src = &pixels[y * row..(y + 1) * row];
        for px in src.chunks_exact(4) {
            out.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    std::fs::write(path, out)
}
