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
// Stark — holographic arc reactor. Cyan rings on a dark grid, gold core,
// slow sweep. Built for the Iron Man theme; reads well as a background.
fragment float4 f_main(VOut in [[stage_in]], constant Uniforms &u [[buffer(0)]]) {
    float2 res = u.resolution;
    float2 uv  = in.uv;
    float2 p   = (uv - 0.5) * float2(res.x / max(res.y, 1.0), 1.0);
    float  t   = u.time;

    float3 col = float3(0.012, 0.020, 0.030);

    // faint engineering grid
    float2 g = abs(fract(p * 12.0) - 0.5);
    float grid = smoothstep(0.470, 0.500, max(g.x, g.y));
    col += float3(0.020, 0.090, 0.130) * grid * 0.55;

    float r = length(p);
    float a = atan2(p.y, p.x);

    // reactor core
    float core = exp(-r * 9.0);
    col += float3(0.55, 0.42, 0.12) * core * 1.55;
    col += float3(0.10, 0.55, 0.75) * exp(-r * 3.2) * 0.50;

    // concentric rings, each on its own slow pulse
    for (int i = 0; i < 4; i++) {
        float fi = float(i);
        float rr = 0.155 + fi * 0.085;
        float w  = 0.0035 + 0.0015 * fi;
        float ring = smoothstep(w, 0.0, abs(r - rr));
        float pulse = 0.55 + 0.45 * sin(t * (1.1 + fi * 0.35) - fi);
        col += float3(0.10, 0.62, 0.85) * ring * pulse * (1.0 - fi * 0.15);
    }

    // rotating spokes between core and first ring
    float spokes = pow(max(0.0, cos(a * 10.0 + t * 0.35)), 12.0);
    spokes *= smoothstep(0.62, 0.18, r) * smoothstep(0.10, 0.18, r);
    col += float3(0.08, 0.45, 0.65) * spokes * 0.55;

    // scan sweep
    float sweep = exp(-pow((uv.y - fract(t * 0.08)) * 40.0, 2.0));
    col += float3(0.05, 0.25, 0.35) * sweep * 0.35;

    // scanlines + vignette
    col *= 0.92 + 0.08 * sin(uv.y * res.y * 1.4);
    col *= 1.0 - smoothstep(0.55, 1.15, length((uv - 0.5) * 1.25));

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
        Ok(format!(
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
        ))
    }
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
