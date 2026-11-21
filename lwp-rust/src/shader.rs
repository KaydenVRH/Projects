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
// Stark — a hologram, not a machine. Thin glowing wireframe rings with a
// chromatic edge, a tick ring, a radar sweep, crosshairs, corner brackets and
// data bars, all projected on a dim grid and washed with scanlines.
static float hash21(float2 p) {
    return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
}

fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float2 uv  = in.uv;
    float2 p   = (uv - 0.5) * float2(res.x / max(res.y, 1.0), 1.0) * 2.0;
    float t    = u.time;

    float r = length(p);
    float a = atan2(p.y, p.x);

    float3 cyan = float3(0.22, 0.82, 1.00);
    float3 ice  = float3(0.88, 0.97, 1.00);

    float3 col = float3(0.004, 0.009, 0.015);

    // projection flicker: two detuned sines, so it never repeats visibly
    float flick = 0.93 + 0.07 * sin(t * 6.7) * sin(t * 2.3);

    // ---- projection grid
    float2 gf = abs(fract(p * 4.0) - 0.5);
    float grid = smoothstep(0.480, 0.500, max(gf.x, gf.y));
    col += float3(0.030, 0.100, 0.140) * grid * (1.0 - smoothstep(0.2, 1.9, r)) * 1.25;

    // ---- wireframe rings; each channel samples a slightly different radius,
    // which is what gives a projector its colour fringe
    for (int i = 0; i < 4; i++) {
        float fi = float(i);
        float R  = 0.20 + fi * 0.135;
        float w  = 0.0034 + fi * 0.0010;
        float lr = 1.0 - smoothstep(0.0, w, abs(r * 1.0025 - R));
        float lg = 1.0 - smoothstep(0.0, w, abs(r         - R));
        float lb = 1.0 - smoothstep(0.0, w, abs(r * 0.9975 - R));
        col += float3(lr, lg, lb) * cyan * 0.95;
        col += cyan * exp(-abs(r - R) * 34.0) * 0.55;
    }

    // ambient bloom: the projection lights the air around itself
    col += cyan * exp(-r * 2.2) * 0.055;

    // ---- tick ring
    float segA = 6.2831853 / 64.0;
    float ta   = floor(a / segA + 0.5) * segA;
    float arc  = abs(a - ta) * max(r, 1e-4);
    float tick = 1.0 - smoothstep(0.0, 0.0085, arc);
    float band = smoothstep(0.470, 0.490, r) * (1.0 - smoothstep(0.520, 0.540, r));
    col += cyan * tick * band * 0.55;

    float segB = 6.2831853 / 12.0;
    float tb   = floor(a / segB + 0.5) * segB;
    float arcB = abs(a - tb) * max(r, 1e-4);
    float tickB = 1.0 - smoothstep(0.0, 0.011, arcB);
    float bandB = smoothstep(0.600, 0.620, r) * (1.0 - smoothstep(0.660, 0.680, r));
    col += cyan * tickB * bandB * 0.40;

    // ---- radar sweep
    float sw = fract(t * 0.09) * 6.2831853 - 3.14159;
    float da = abs(atan2(sin(a - sw), cos(a - sw)));
    col += cyan * exp(-da * 5.0) * smoothstep(0.62, 0.10, r) * 0.16;

    // ---- crosshair
    col += cyan * exp(-abs(p.y) * 240.0) * smoothstep(0.95, 0.25, r) * 0.40;
    col += cyan * exp(-abs(p.x) * 240.0) * smoothstep(0.95, 0.25, r) * 0.40;

    // ---- core
    col += ice  * exp(-r * 26.0) * 1.15;
    col += cyan * exp(-r * 6.0)  * 0.22;

    // ---- data bars, right side
    for (int i = 0; i < 5; i++) {
        float fi = float(i);
        float y  = -0.50 + fi * 0.075;
        float x1 = 0.60 + 0.22 * (0.35 + 0.65 * hash21(float2(fi, floor(t * 0.4))));
        float bar = smoothstep(0.600, 0.605, p.x) * (1.0 - smoothstep(x1, x1 + 0.005, p.x))
                  * smoothstep(y, y + 0.006, p.y) * (1.0 - smoothstep(y + 0.018, y + 0.024, p.y));
        col += cyan * bar * 0.30;
    }

    // ---- corner brackets
    float2 cq  = abs(p) - float2(1.05, 0.72);
    float  adx = abs(cq.x);
    float  ady = abs(cq.y);
    float  armH = (1.0 - smoothstep(0.0, 0.006, ady)) * (1.0 - smoothstep(0.18, 0.19, adx));
    float  armV = (1.0 - smoothstep(0.0, 0.006, adx)) * (1.0 - smoothstep(0.10, 0.11, ady));
    col += cyan * (armH + armV) * 0.45;

    // ---- a scan band travelling down the projection
    float scan = exp(-pow((uv.y - fract(t * 0.05)) * 26.0, 2.0));
    col += cyan * scan * 0.05 * (1.0 - smoothstep(0.9, 1.6, r));

    // ---- scanlines, grain, vignette
    col *= 0.90 + 0.10 * sin(uv.y * res.y * 1.2);
    col += (hash21(uv * res + t) - 0.5) * 0.010;
    col *= 1.0 - smoothstep(0.85, 1.90, r);
    col *= flick;

    return float4(col, 1.0);
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
    let (msg, _buf) = check_with_pixels(source, width, height)?;
    Ok(msg)
}

/// Same as `check`, plus the raw BGRA pixels so a caller can look at the frame.
pub fn check_with_pixels(
    source: &str,
    width: u32,
    height: u32,
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
            time: 1.0,
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

    let scale = 2.0f64;
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

        while !SHOULD_QUIT.load(Ordering::SeqCst) {
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
