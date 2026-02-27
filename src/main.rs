use std::{
    iter,
    cell::RefCell,
    collections::VecDeque,
    f32::consts::FRAC_PI_2,
};
use nannou::{
    prelude::*,
    geom::Tri,
    glam::{Vec3Swizzles, Vec4Swizzles},
    color::IntoLinSrgba,
    rand::{RngCore, rngs::SmallRng, SeedableRng},
    ease,
};
use nannou_audio as audio;
use glicol;
use audrey;

// Design ratio as
// x/y. On window mismatch,
// fit it to the screen.
//
// ("Non-square" scale, i.e., -1 to 1 both horizontally and vertically, would mess up stroke width
// rendering, square sides of 1.0, etc. for a non-square window. Best effort. We'll want height as
// -1 to 1 and width as AR, so that an unit circle centered on the screen does not clip.
const AR: f32 = 16.0 / 9.0;

struct Graphics {
    uniform_buffer: wgpu::Buffer,
    tris_buffer: wgpu::Buffer,
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    position: (f32, f32, f32),
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Uniforms {
    world: Mat4,
    iworld: Mat4,
    view: Mat4,
    proj: Mat4,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Tris {
    vertices: [Vertex; 2 * 24 * 3],
    colors: [Vertex; 2 * 24],
}

type GliEngine = glicol::Engine::<128>;

struct AudioModel {
    gli: GliEngine,
    samples: VecDeque<f32>,
}

struct Model {
    _window_id: window::Id,
    scroll: f32,
    graphics: RefCell<Graphics>,
    _astream: audio::Stream<AudioModel>,
}

fn main() {
    nannou::app(model)
        .update(update)
        .run();
}

fn graphics(app: &App, window_id: window::Id) -> Graphics {
    let window = app.window(window_id).expect("where did our window go?");
    let device = window.device();
    let format = Frame::TEXTURE_FORMAT;
    let depth_format = wgpu::TextureFormat::Depth32Float;
    let msaa_samples = window.msaa_samples();
    let (win_w, win_h) = window.inner_size_pixels();

    let vs_desc = wgpu::include_wgsl!("../shaders/vs.wgsl");
    let fs_desc = wgpu::include_wgsl!("../shaders/fs.wgsl");
    let vs_mod = device.create_shader_module(vs_desc);
    let fs_mod = device.create_shader_module(fs_desc);

    let depth_texture = create_depth_texture(device, [win_w, win_h], depth_format, msaa_samples);
    let depth_texture_view = depth_texture.view().build();

    let uniforms = create_uniforms(0.0, [win_w, win_h]);
    let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: None,
        contents: uniforms_as_bytes(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let tris = Tris {
        vertices: [Vertex { position: (0.0, 0.0, 0.0) }; _],
        colors: [Vertex { position: (0.0, 0.0, 0.0) }; _],
    };
    let tris_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: None,
        contents: tris_as_bytes(&tris),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });

    let bind_group_layout = create_bind_group_layout(device);
    let bind_group = create_bind_group(device, &bind_group_layout, &uniform_buffer, &tris_buffer);
    let pipeline_layout = create_pipeline_layout(device, &bind_group_layout);
    let render_pipeline = create_render_pipeline(
        device,
        &pipeline_layout,
        &vs_mod,
        &fs_mod,
        format,
        depth_format,
        msaa_samples,
    );

    Graphics {
        uniform_buffer,
        tris_buffer,
        depth_texture,
        depth_texture_view,
        bind_group,
        render_pipeline,
    }
}

fn load_wav(filename: &str) -> Vec<f32> {
    let mut reader = audrey::open(filename).expect("where sound file?");
    // NB! this channel count must match the file or it plays too fast or too slow
    reader.frames::<[f32; 1]>()
        .filter_map(Result::ok)
        .map(|f| f[0])
        .collect()
}

fn init_glicol() -> GliEngine {
    let mut gli = GliEngine::new();
    for sname in ["dum1"] {
        let samp = Box::new(load_wav(&format!("audio/{}.wav", sname)));
        gli.add_sample(&format!("\\{}", sname), Box::leak(samp), 1, 44100);
    }
    gli.update_with_code(include_str!("../music.glicol"));
    gli
}

fn init_audio() -> audio::Stream<AudioModel> {
    let audio_host = audio::Host::new();
    let gli = init_glicol();
    let mut amodel = AudioModel { gli, samples: VecDeque::new() };

    // TODO some day
    let _sr = audio_host.default_output_device()
        .expect("No audio output devices?")
        .default_output_config()
        .expect("No audio output stream config?")
        .sample_rate().0 as usize;
    amodel.gli.set_sr(44100); // that's the default as of writing but just to be safe

    let astream = audio_host
        .new_output_stream(amodel)
        .sample_rate(44100)
        .render(audio)
        .build()
        .expect("cannot make audio output stream");
    astream.play().expect("cannot play audio stream");
    astream
}

fn model(app: &App) -> Model {
    let window_id = app.new_window()
        .size(1920, 1080)
        .view(view)
        .mouse_wheel(mouse_wheel)
        .fullscreen()
        .build()
        .expect("cannot build app window");

    let graphics = RefCell::new(graphics(app, window_id));

    let astream = init_audio();

    Model {
        _window_id: window_id,
        scroll: 14.0,
        graphics,
        _astream: astream,
    }
}

fn audio(audio: &mut AudioModel, buffer: &mut audio::Buffer) {
    for frame in buffer.frames_mut() {
        if audio.samples.is_empty() {
            let (block, err) = audio.gli.next_block(vec![]);
            if err[0] != 0 {
                let buf = Vec::from(&err[1..]);
                match String::from_utf8(buf) {
                    Ok(msg) => {
                        println!("glicol audio error! {msg}");
                    },
                    Err(e) => {
                        println!("glicol audio error but cannot parse it! {e}");
                    }
                }
            }
            for &sample in block[0].iter() {
                audio.samples.push_back(sample);
            }
        }
        let sample = audio.samples.pop_front().expect("no chance");
        for channel in frame {
            *channel = sample;
        }
    }
}

fn mouse_wheel(_app: &App, model: &mut Model, dt: MouseScrollDelta, _phase: TouchPhase) {
    match dt {
        MouseScrollDelta::LineDelta(_h, v) => {
            model.scroll += v;
            println!("scroll {}", model.scroll);
        },
        MouseScrollDelta::PixelDelta(_) => {},
    }
}

fn update(_app: &App, _model: &mut Model, _update: Update) {
}

// first some aliases for consistency
#[allow(dead_code)]
fn rotation_xy(angle: f32) -> Mat4 {
    Mat4::from_rotation_z(angle)
}

#[allow(dead_code)]
fn rotation_xz(angle: f32) -> Mat4 {
    Mat4::from_rotation_y(angle)
}

#[allow(dead_code)]
fn rotation_yz(angle: f32) -> Mat4 {
    Mat4::from_rotation_x(angle)
}

#[allow(dead_code)]
fn rotation_xw(angle: f32) -> Mat4 {
    let xy = rotation_xy(angle);
    Mat4::from_cols(
        xy.col(0).xzwy(),
        xy.col(2).xzwy(),
        xy.col(3).xzwy(),
        xy.col(1).xzwy())
}

#[allow(dead_code)]
fn rotation_yw(angle: f32) -> Mat4 {
    let xy = rotation_xy(angle);
    Mat4::from_cols(
        xy.col(3).wxzy(),
        xy.col(0).wxzy(),
        xy.col(2).wxzy(),
        xy.col(1).wxzy())
}

#[allow(dead_code)]
fn rotation_zw(angle: f32) -> Mat4 {
    let xy = rotation_xy(angle);
    Mat4::from_cols(
        xy.col(2).zwxy(),
        xy.col(3).zwxy(),
        xy.col(0).zwxy(),
        xy.col(1).zwxy())
}

fn geometry(app: &App, model: &Model, time: f32) -> (Vec<Vertex>, Vec<Vertex>) {
    // this could be just bit patterns from 0 to 15
    let verts = [
        // bottom
        vec4(-1.0, -1.0, -1.0, -1.0),
        vec4( 1.0, -1.0, -1.0, -1.0),
        vec4(-1.0,  1.0, -1.0, -1.0),
        vec4( 1.0,  1.0, -1.0, -1.0),
        // top
        vec4(-1.0, -1.0,  1.0, -1.0),
        vec4( 1.0, -1.0,  1.0, -1.0),
        vec4(-1.0,  1.0,  1.0, -1.0),
        vec4( 1.0,  1.0,  1.0, -1.0),
        // bottom
        vec4(-1.0, -1.0, -1.0,  1.0),
        vec4( 1.0, -1.0, -1.0,  1.0),
        vec4(-1.0,  1.0, -1.0,  1.0),
        vec4( 1.0,  1.0, -1.0,  1.0),
        // top
        vec4(-1.0, -1.0,  1.0,  1.0),
        vec4( 1.0, -1.0,  1.0,  1.0),
        vec4(-1.0,  1.0,  1.0,  1.0),
        vec4( 1.0,  1.0,  1.0,  1.0),
    ];
    let clra = LIGHTBLUE;
    let clrb = MEDIUMPURPLE;
    let clrc = FLORALWHITE;
    let colors = [
        clra,
        clra,
        clra,
        clra,
        clra,
        clra,

        clrb,
        clrb,
        clrb,
        clrb,
        clrb,
        clrb,

        clrc,
        clrc,
        clrc,
        clrc,
        clrc,
        clrc,
        clrc,
        clrc,
        clrc,
        clrc,
        clrc,
        clrc,
    ];
    let colors: [LinSrgba; _] = colors.map(|c| c.into_lin_srgba());
    /*
     * 2 6 (-1, 1)  3 7 (1,  1)
     *
     * 0 4 (-1,-1)  1 5 (1, -1)
     */

    // just a 3d to start with
    // faces as viewed from the outside, hopefully consistent "Z" winding
    // but "left" and "right" as viewed from the front, not that particular face
    let quads = [
        (0, 1, 2, 3), // bot
        (6, 7, 4, 5), // top
        (4, 5, 0, 1), // front
        (7, 6, 3, 2), // back
        (6, 4, 2, 0), // left
        (5, 7, 1, 3), // right
    ];
    let quads2 = quads.iter().map(|q| (8 + q.0, 8 + q.1, 8 + q.2, 8 + q.3)).collect::<Vec<_>>();
    // look at a picture of a cube inside a cube to see this; outer has w=-1, inner w=1
    // what's the winding or orientation with these? split across two 4D coords, depends on viewpoint
    let quads3 = [
        (0, 1, 8, 9), // bottom front
        (3, 2, 11, 10), // bottom back
        (2, 0, 10, 8), // bottom left
        (1, 3, 9, 11), // bottom right

        (4, 5, 12, 13), // top front
        (7, 6, 15, 14), // top back
        (6, 4, 14, 12), // top left
        (5, 7, 13, 15), // top right

        (4, 12, 0, 8), // front left
        (5, 13, 1, 9), // front right

        (6, 14, 2, 10), // back left
        (7, 15, 3, 11), // back right
    ];

    // let lw = 1.1 + 0.1 * model.scroll.abs();
    let lw = 1.1 + 0.1 * (200.0 + (13.0 - 200.0) * (300.0 * time).min(1.0));  // ease::quint::ease_in((1000.0 * time).max(1.0), 200.0, 13.0, 1.0);
    let project4d = |p: Vec4| (1.0 / (lw - p.w) * p).truncate();
    let rotate4d = |p: Vec4| {
        rotation_xy(app.time) * rotation_zw(1.5*app.time) *
        //rotation_xy(app.mouse.x / 50.0) * rotation_zw(app.mouse.y / 50.0) *
            p
    };

    let v = |i: usize| project4d(rotate4d(verts[i]));
    let qua = quads.iter().chain(quads2.iter()).chain(quads3.iter()).enumerate()
        .map(|(i, q)| {
            ((v(q.0), v(q.1), v(q.2), v(q.3)), colors[i % colors.len()])
        })
    .collect::<Vec<_>>();
    let tri = qua.iter()
        .flat_map(|(q, c)| {
            let c = lin_srgba(c.red, c.green, c.blue, 0.5);
            let a = Tri([(q.0, c), (q.1, c), (q.2, c)]);
            let b = Tri([(q.3, c), (q.2, c), (q.1, c)]);
            iter::once(a).chain(iter::once(b))
        })
    .collect::<Vec<_>>();
    let verts = tri.iter()
        .flat_map(|v| v.map_vertices(|(pt, _color)| pt).vertices())
        .map(|v| Vertex { position: (v.x, v.y, v.z) })
        .collect::<Vec<_>>();
    let colors = tri.iter()
        .flat_map(|v| v.map_vertices(|(_pt, color)| color).vertices().take(1))
        .map(|v| Vertex { position: (v.red, v.green, v.blue) })
        .collect::<Vec<_>>();
    (verts, colors)
}

fn view(app: &App, model: &Model, frame: Frame) {
    frame.clear(PURPLE);
    let scenes: [(fn(&App, &Model, Frame, f32), f32); _] = [
        (view_loading, 2.0),
        (view_dropping, 1.5),
        (view_undrop, 2.5),
        (view_walking, 20.0),
        (view_walkoff, 2.0),
        (view_hyper, 1000.0),
    ];
    let mut runtime = 0.0;
    for (sfunc, stime) in scenes {
        if app.time < runtime + stime {
            let tick = (app.time - runtime) / stime;
            sfunc(app, model, frame, tick);
            break;
        }
        runtime += stime;
    }
}

// Normally, (w,h) square fills the screen. make it ([-AR..AR], [-1..1]) for AR'd screen.
// Normally, stroke weight S is S pixels wide. make it proportional to screen height.
// For non-exact aspect ratio window, have padding on either side in extra dimension.
//
// Can't just scale the viewport to (-1, 1) because line weight would scale as well!
//
// TODO finally clip somehow, maybe just draw them blanks on top lol
//
// - draw is the original
// - d is scaled for viewport
// - s scales vec2(1, 1) to top right corner at vec2(AR, 1)
// - r is the viewport height in pixels, AR * r is viewport width in pixels
fn draw_viewported(app: &App, model: &Model) -> (Draw, Draw, Mat2, f32) {
    let window = app.window(model._window_id).expect("where did our window go?");
    let win_rect = window.rect();
    let arw = win_rect.w() / win_rect.h();
    let draw = app.draw();
    let r = if arw > AR {
        // wide window
        win_rect.h()
        // or, win_rect.w() / arw
    } else {
        // narrow window
        win_rect.w() / AR
        // or, win_rect.h() * arw / AR
    };
    let d = draw.scale(r);
    let s = mat2(vec2(AR, 0.0), vec2(0.0, 1.0));
    (draw, d, s, r)
}

fn view_loading(app: &App, model: &Model, frame: Frame, time: f32) {
    frame.clear(BLACK);
    let (draw, d, s, r) = draw_viewported(app, model);
    let (w, h) = (0.8, 0.1);
    // t, from, distance, duration
    let eas = ease::quad::ease_in(time, 0.0, 1.0, 1.0);
    d.rect()
        .no_fill()
        .stroke_color(WHITE)
        .stroke_weight(0.01)
        .wh(s * vec2(w, h));
    d.rect()
        .color(WHITE)
        .xy(s * vec2(0.5 * w * (eas - 1.0), 0.0))
        .wh(s * vec2(eas * w, h));
    draw.text("the game ;-)     ")
        .color(BLACK)
        .font_size((0.06 * r) as u32)
        .w(w * AR * r)
        .right_justify();

    draw.to_frame(app, &frame).expect("draw fail");
}

fn view_dropping(app: &App, model: &Model, frame: Frame, time: f32) {
    frame.clear(BLACK);

    let (draw, d, s, _r) = draw_viewported(app, model);
    let (w, h) = (0.8, 0.1);
    let wh = vec2(w, h);
    let y = (-0.5 + 0.5 * h) * time * time;
    d.rect()
        .no_fill()
        .stroke_color(WHITE)
        .stroke_weight(0.01)
        .y(y)
        .wh(s * wh);
    d.rect()
        .color(rgb(1.0 - time, 1.0 - time, 1.0 - time))
        .y(y)
        .wh(s * wh);

    draw.to_frame(app, &frame).expect("draw fail");
}

fn view_undrop(app: &App, model: &Model, frame: Frame, time: f32) {
    frame.clear(BLACK);

    let (draw, d, _s, _r) = draw_viewported(app, model);

    mountain_landscape(&d, time, 0.0);

    let sz = 0.11;
    let ypos = -0.2;
    let xpos = -0.5;

    let emit = 1.0 - time;
    let ew = ease::cubic::ease_in_out(time, AR * 0.8, sz - AR * 0.8, 1.0);
    let eh = ease::cubic::ease_in_out(time, 0.1, sz - 0.1, 1.0);
    let (w, h) = (ew, eh);
    let wh = vec2(w, h);
    let xdest = xpos - 0.5*sz;
    let ydest = ypos + 0.5*sz;
    let y0 = -0.5 + 0.5 * 0.1;
    let x = ease::cubic::ease_in_out(time, 0.0, xdest, 1.0);
    let y = ydest + (y0 - ydest) * emit * emit;
    let rot = ease::back::ease_in_out(time, 0.0, FRAC_PI_2, 1.0);
    let xy = vec2(x, y);
    d.rect()
        .no_fill()
        .stroke_color(WHITE)
        .stroke_weight(0.01)
        .z_radians(rot)
        .xy(xy)
        .z(1.0)
        .wh(wh);

    draw.to_frame(app, &frame).expect("draw fail");
}

fn mountain_landscape(d: &Draw, time: f32, xoff: f32) {
    let sqsz = 1.8;
    let mountain_distance = 20.0; // square units
    let ground_y = -4.0; // camera at 0
    // grid

    let proj = Mat4::perspective_rh_gl(FRAC_PI_2, AR, 0.01, 1.0e4);
    let eye = pt3(xoff, 0.0, 0.0);
    let target = pt3(xoff, 0.0, -1.0);
    let view = Mat4::look_at_rh(eye, target, Vec3::Y);
    let pv = proj * view;
    // "transform"
    let xf = |p: Vec3| {
        let a = pv * p.extend(1.0);
        (1.0 / a.w * a).xyz()
    };
    let near = -1.0;
    let far = -mountain_distance * sqsz;
    let near = map_range(time, 0.0, 1.0, far, near);
    let horiz = ((((1.0 - time) * mountain_distance) as i32)..=mountain_distance as i32).map(|i| {
        let z = -sqsz * i as f32;
        (vec3(-50.0, ground_y, z),
         vec3( 50.0, ground_y, z))
    });
    let vert = (-50..50).map(|i| {
        let x = sqsz * i as f32;
        (vec3(x, ground_y, near),
         vec3(x, ground_y, far))
    });
    for (a, b) in horiz.chain(vert) {
        d.line()
            .start(xf(a).xy())
            .end(xf(b).xy())
            .weight(0.001)
            .color(MAGENTA);
    }

    // mountains

    // TODO bright edges might make this better
    // TODO bright blue horizon thing
    let mut rng = SmallRng::seed_from_u64(31337);
    let mut rnd = || rng.next_u32() as f32 / std::u32::MAX as f32;
    let mut rndb = |prob: f32| rnd() < prob;
    for &sz in [1i32, 3, 5, 11, 17, 23].iter() {
        let geom = (-100..100).step_by(sz as usize).filter(|&_| rndb(0.8))
            .map(|i| {
                let fx = |x: i32| (sz + x * sz) as f32;
                let x0 = fx(i);
                let x1 = fx(i + 2);
                let y0 = ground_y;
                let y1 = ground_y + time * 0.9 * sz as f32;
                let c = rgba(1.0, 0.0, 1.0, 0.2);
                Tri::from_vertices([
                    (xf(vec3(x0, y0, -mountain_distance * sqsz)), c),
                    (xf(vec3(0.5 * (x0 + x1), y1, -mountain_distance * sqsz)), c),
                    (xf(vec3(x1, y0, -mountain_distance * sqsz)), c)
                ]).expect("3 is not 3 in this universe")
            });
        d
            .mesh()
            .tris_colored(geom);
    }
}

fn view_walking(app: &App, model: &Model, frame: Frame, time: f32) {
    walking(app, model, frame, time, 0.0);
}

fn walking(app: &App, model: &Model, frame: Frame, time: f32, time2: f32) {
    frame.clear(BLACK);

    let (draw, d, _s, r) = draw_viewported(app, model);

    let tim = 1.0 * 20.0 * time;
    let tick = tim * FRAC_PI_2;
    let ftick = tick % FRAC_PI_2;
    let itick = (tick - ftick) / FRAC_PI_2;

    let sz = 0.11;
    let ypos = -0.2;
    let xpos = -0.5;

    let follow_rate = ease::sine::ease_out(time, 0.0, 1.0, 3.5);

    let d = d.y(-4.0 * time2);
    mountain_landscape(&d, 1.0, 3.0 * tim * sz);
    let d = d.y(8.0 * time2);

    d
        .x(0.9 * follow_rate * -tim * sz)
        .x(itick * sz + xpos)
        .y(ypos)
        .z_radians(-ftick)
        .translate(vec3(-0.5 * sz, 0.5 * sz, 1.0))
        .rect()
        .no_fill()
        .stroke_color(WHITE)
        .stroke_weight(0.01)
        .wh(vec2(sz, sz));

    let msg = "get ready for 2026-06-05 to 2026-06-07 ~ grab snacks and hack around ~ finish a demo ~ win the compo ~ ??? ~ profit";
    let t = -3.14*1.5 * time;
    for (i, ch) in msg.chars().enumerate() {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        let x = r * (0.4 * AR + 0.03 * i as f32 + t);
        let dy = (0.01 * x).sin() * 0.1;
        draw
            .z(1.0)
            .x(x - time2 * r * AR)
            .y((-0.30 + dy) * r)
            .text(s)
            .color(MEDIUMPURPLE)
            .font_size((0.05 * r) as u32)
            ;
    }

    draw.to_frame(app, &frame).expect("draw fail");
}

fn view_walkoff(app: &App, model: &Model, frame: Frame, time: f32) {
    frame.clear(BLACK);

    let zip = ease::expo::ease_in(time, 0.0, 1.0, 1.0);
    walking(app, model, frame, 1.0, zip);
}

fn view_hyper(app: &App, model: &Model, frame: Frame, time: f32) {
    let mut g = model.graphics.borrow_mut();

    let frame_size = frame.texture_size();
    let device = frame.device_queue_pair().device();
    if frame_size != g.depth_texture.size() {
        let depth_format = g.depth_texture.format();
        let sample_count = frame.texture_msaa_samples();
        g.depth_texture = create_depth_texture(device, frame_size, depth_format, sample_count);
        g.depth_texture_view = g.depth_texture.view().build();
    }

    let uniforms = create_uniforms(time, frame_size);
    let new_uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: None,
        contents: uniforms_as_bytes(&uniforms),
        usage: wgpu::BufferUsages::COPY_SRC,
    });

    let (geom, colors) = geometry(app, model, time);
    let mut tris = Tris {
        vertices: [Vertex { position: (0.0, 0.0, 0.0) }; _],
        colors: [Vertex { position: (1.0, 0.0, 0.0) }; _],
    };
    tris.vertices.copy_from_slice(&geom);
    tris.colors.copy_from_slice(&colors);
    let new_tris_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: None,
        contents: tris_as_bytes(&tris),
        usage: wgpu::BufferUsages::COPY_SRC,
    });

    let vert_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: None,
        contents: vertices_as_bytes(&geom),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let mut encoder = frame.command_encoder();
    let uniforms_size = std::mem::size_of::<Uniforms>() as wgpu::BufferAddress;
    let tris_size = std::mem::size_of::<Tris>() as wgpu::BufferAddress;
    encoder.copy_buffer_to_buffer(&new_uniform_buffer, 0, &g.uniform_buffer, 0, uniforms_size);
    encoder.copy_buffer_to_buffer(&new_tris_buffer, 0, &g.tris_buffer, 0, tris_size);

    let mut render_pass = wgpu::RenderPassBuilder::new()
        .color_attachment(frame.texture_view(), |color| color)
        .depth_stencil_attachment(&g.depth_texture_view, |depth| depth)
        .begin(&mut encoder);
    render_pass.set_bind_group(0, &g.bind_group, &[]);
    render_pass.set_pipeline(&g.render_pipeline);
    render_pass.set_vertex_buffer(0, vert_buffer.slice(..));
    render_pass.draw(0..geom.len() as u32, 0..1);
}

fn create_uniforms(apptime: f32, [w, h]: [u32; 2]) -> Uniforms {
    let rotation = Mat4::from_rotation_y(0.5 * FRAC_PI_2);
    let fov_y = std::f32::consts::FRAC_PI_2;
    let proj = Mat4::perspective_rh_gl(fov_y, w as f32 / h as f32, 0.01, 100.0);
    let eye = pt3(0.3, 0.3, 2.5);
    let target = Point3::ZERO;
    let up = Vec3::Y;
    let view = Mat4::look_at_rh(eye, target, up);
    let scale = Mat4::from_scale(Vec3::splat(1.0));
    Uniforms {
        world: rotation,
        iworld: rotation.inverse(),
        view: (view * scale).into(),
        proj: proj.into(),
    }
}

fn create_depth_texture(
    device: &wgpu::Device,
    size: [u32; 2],
    depth_format: wgpu::TextureFormat,
    sample_count: u32,
) -> wgpu::Texture {
    wgpu::TextureBuilder::new()
        .size(size)
        .format(depth_format)
        .usage(wgpu::TextureUsages::RENDER_ATTACHMENT)
        .sample_count(sample_count)
        .build(device)
}

fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    wgpu::BindGroupLayoutBuilder::new()
        .uniform_buffer(wgpu::ShaderStages::VERTEX_FRAGMENT, false)
        .storage_buffer(wgpu::ShaderStages::FRAGMENT, false, false)
        .build(device)
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    tris_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    wgpu::BindGroupBuilder::new()
        .buffer::<Uniforms>(uniform_buffer, 0..1)
        .buffer::<Tris>(tris_buffer, 0..1)
        .build(device, layout)
}

fn create_pipeline_layout(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::PipelineLayout {
    let desc = wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    };
    device.create_pipeline_layout(&desc)
}

fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    vs_mod: &wgpu::ShaderModule,
    fs_mod: &wgpu::ShaderModule,
    dst_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    sample_count: u32,
) -> wgpu::RenderPipeline {
    wgpu::RenderPipelineBuilder::from_layout(layout, vs_mod)
        .fragment_shader(&fs_mod)
        .color_format(dst_format)
        .color_blend(wgpu::BlendComponent::REPLACE)
        .alpha_blend(wgpu::BlendComponent::REPLACE)
        .add_vertex_buffer::<Vertex>(&wgpu::vertex_attr_array![0 => Float32x3])
        .depth_format(depth_format)
        .sample_count(sample_count)
        .build(device)
}

// see nannou::wgpu::bytes docs, but bytemuch might be suitable today?

fn vertices_as_bytes(data: &[Vertex]) -> &[u8] {
    unsafe { wgpu::bytes::from_slice(data) }
}

fn uniforms_as_bytes(uniforms: &Uniforms) -> &[u8] {
    unsafe { wgpu::bytes::from(uniforms) }
}

fn tris_as_bytes(tris: &Tris) -> &[u8] {
    unsafe { wgpu::bytes::from(tris) }
}
