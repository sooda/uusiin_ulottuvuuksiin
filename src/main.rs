use nannou::prelude::*;
use nannou::geom::Tri;
use nannou::glam::Vec4Swizzles;
use nannou::color::IntoLinSrgba;
use nannou_audio as audio;
use std::iter;
use std::cell::RefCell;
use glicol;
use std::collections::VecDeque;

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

fn model(app: &App) -> Model {
    let window_id = app.new_window()
        .size(960, 540)
        .view(view)
        .mouse_wheel(mouse_wheel)
        .build()
        .expect("cannot build app window");

    let graphics = RefCell::new(graphics(app, window_id));

    let audio_host = audio::Host::new();
    let mut gli = GliEngine::new();
    gli.update_with_code(r#"bass: speed 2.0 >> seq 40_40 45 50 >> sawsynth 0.01 0.1"#);

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
            let block = audio.gli.next_block(vec![]);
            for &sample in block.0[0].iter() {
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

fn geometry(app: &App, model: &Model) -> (Vec<Vertex>, Vec<Vertex>) {
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

    // just to a "4D plane" (3D volume) for now
    //let project4d = |p: Vec4| vec3(p.x, p.y, p.z);
    let lw = 1.1 + 0.1 * model.scroll.abs();
    let project4d = |p: Vec4| (1.0 / (lw - p.w) * p).truncate();
    let rotate4d = |p: Vec4| {
        //rotation_xy(app.time) * rotation_zw(1.5*app.time)
        rotation_xy(app.mouse.x / 50.0) * rotation_zw(app.mouse.y / 50.0)
            * p
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
        (view_loading, 0.5),
        (view_dropping, 0.5),
        (view_walking, 100.0),
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

fn view_loading(app: &App, model: &Model, frame: Frame, time: f32) {
    frame.clear(BLACK);

    let window = app.window(model._window_id).expect("where did our window go?");
    let win_rect = window.rect();
    let draw = app.draw();
    {
        // non-square scale would mess up stroke rendering. best effort.
        // TODO: or scale based on min(width, height) and have blank in the too wide direction
        let draw = draw
            .scale_x(win_rect.w())
            .scale_y(win_rect.w());
        draw.rect()
            .no_fill()
            .stroke_color(WHITE)
            .stroke_weight(0.01)
            .wh(vec2(0.8, 0.10));
        draw.rect()
            .color(WHITE)
            .x(0.5 * 0.8 * (time - 1.0))
            .wh(vec2(time * 0.8, 0.10));
    }
    draw.text("the game ;-)  ")
        .color(BLACK)
        .font_size(50 * win_rect.h() as u32 / 540)
        .w(0.8 * win_rect.w())
        .h(win_rect.h())
        .justify(nannou::text::Justify::Right)
        .align_text_middle_y();

    draw.to_frame(app, &frame).expect("draw fail");
}

fn view_dropping(app: &App, model: &Model, frame: Frame, time: f32) {
    frame.clear(BLACK);

    let window = app.window(model._window_id).expect("where did our window go?");
    let win_rect = window.rect();
    let draw = app.draw();
    {
        let draw = draw
            .scale_x(win_rect.w())
            .scale_y(win_rect.w());
        draw.rect()
            .no_fill()
            .stroke_color(WHITE)
            .stroke_weight(0.01)
            .y(-0.5 * time * time * 540.0 / 960.0)
            .wh(vec2(0.8, 0.10));
        draw.rect()
            .color(rgb(1.0 - time, 1.0 - time, 1.0 - time))
            // FIXME aspect ratio
            .y(-0.5 * time * time * 540.0 / 960.0)
            .wh(vec2(0.8, 0.10));
    }

    draw.to_frame(app, &frame).expect("draw fail");
}

fn view_walking(app: &App, model: &Model, frame: Frame, time: f32) {
    frame.clear(BLACK);

    let window = app.window(model._window_id).expect("where did our window go?");
    let win_rect = window.rect();
    let draw = app.draw();
    {
        // grid
        {
            let draw = draw
                .scale_x(win_rect.h())
                .scale_y(win_rect.h());
            let aspect_ratio = win_rect.w() as f32 / win_rect.h() as f32;
            let fov_y = std::f32::consts::FRAC_PI_2;
            let near = 0.01;
            let far = 10000.0;
            let proj = Mat4::perspective_rh_gl(fov_y, aspect_ratio, near, far);
            let eye = pt3(0.0, 0.0, 1.0);
            let target = pt3(0.0, 0.0, 0.0);
            let up = Vec3::Y;
            let view = Mat4::look_at_rh(eye, target, up);
            let pv = proj * view;
            let sqsz = 0.8;
            let horiz = (0..100).map(|i| {
                let z = sqsz * i as f32;
                (vec4(-100.0,  1.0, z, 1.0),
                 vec4( 100.0,  1.0, z, 1.0))
            });
            let vert = (-100..100).map(|i| {
                let x = sqsz * i as f32;
                (vec4(x, -1.0, -2.0, 1.0),
                 vec4(x, -1.0, 1.0e6, 1.0))
            });
            for (a, b) in horiz.chain(vert) {
                let a = pv * a;
                let b = pv * b;
                draw.line()
                    .start(1.0 / a.w * a.xy())
                    .end(1.0 / b.w * b.xy())
                    .weight(0.001)
                    .color(MAGENTA);
            }
        }
        // mountains
        {
            let rnd = |i: f32| ((i * 37.0).sin() * 71551.0).sin();
            let rndb = |i: f32| rnd(i) > 0.0;
            let draw = draw
                .scale_x(win_rect.w())
                .scale_y(win_rect.h())
                .y(-0.00)
                ;
            for &sz in [10, 20, 50, 100].iter() {
                let geom = (0..2*sz).filter(|&i| rndb(i as f32))
                    .map(|i| {
                        let fx = |x: i32| -1.0 + 2.0 * (x as f32 / sz as f32) / 2.0;
                        let x0 = fx(i);
                        let x1 = fx(i + 2);
                        //let y0 = 1.0 / sz as f32;
                        let y0 = 0.0;
                        let y1 = 2.0 / sz as f32;
                        //let y1 = 1.0 / sz as f32;
                        let c = rgba(1.0, 0.0, 1.0, 0.2);
                        Tri::from_vertices([
                            (vec3(x0, y0, 0.0), c),
                            (vec3((x0 + x1) / 2.0, y1, 0.0), c),
                            (vec3(x1, y0, 0.0), c)
                        ]).expect("3 is not 3 in this universe")
                    });
                draw.mesh().tris_colored(geom);
            }
            let fix_aliasing = 0.03;
            draw.rect()
                .x(0.0).y(-fix_aliasing / 2.0)
                .w(1.0).h(fix_aliasing)
                .color(MAGENTA)
                ;
        }

        let pi2 = std::f32::consts::FRAC_PI_2;
        let tick = 1.1*100.0 * time * pi2;
        let ftick = tick % pi2;
        let itick = (tick - ftick) / pi2;

        let sz = 0.2;
        let ypos = -0.2;
        let xpos = -0.5;

        let draw = draw
            .scale_x(win_rect.h())
            .scale_y(win_rect.h());
        draw
            .x(itick * sz + xpos)
            .y(ypos)
            .z_radians(-ftick)
            .translate(vec3(-0.5 * sz, 0.5 * sz, 0.0))
            .rect()
            .no_fill()
            .stroke_color(WHITE)
            .stroke_weight(0.01)
            .wh(vec2(sz, sz));
    }

    draw.to_frame(app, &frame).expect("draw fail");
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

    let (geom, colors) = geometry(app, model);
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
    let rotation = Mat4::from_rotation_y(0.2 * apptime);
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
