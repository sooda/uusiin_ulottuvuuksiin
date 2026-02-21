use nannou::prelude::*;
use nannou::geom::Tri;
use std::iter;

fn main() {
    nannou::app(model)
        .update(update)
        .run();
}

struct Model {
    window_id: window::Id,
    scroll: f32,
}

fn model(app: &App) -> Model {
    let window_id = app.new_window()
        .size(512, 512)
        .view(view)
        .mouse_wheel(mouse_wheel)
        .build()
        .unwrap();

    Model { window_id, scroll: 14.0 }
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

fn view(app: &App, model: &Model, frame: Frame){
    frame.clear(PURPLE);

    let window = app.window(model.window_id).unwrap();
    let win_rect = window.rect();
    let draw = app.draw();
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

    let v = |i: usize| project4d(verts[i]);
    let mut tri = quads.iter().chain(quads2.iter()).chain(quads3.iter())
        .flat_map(|q| {
            let mid = 0.25 * (v(q.0) + v(q.1) + v(q.2) + v(q.3));
            let p = 0.5 * (mid + Vec3::splat(1.0));
            let c = lin_srgba(p.x, p.y, p.z, 0.2); // FIXME colors in 4d
            let a = Tri([(v(q.0), c), (v(q.1), c), (v(q.2), c)]);
            let b = Tri([(v(q.3), c), (v(q.2), c), (v(q.1), c)]);
            iter::once(a).chain(iter::once(b))
        })
    .collect::<Vec<_>>();

    let scale = win_rect.w().min(win_rect.h()) * 0.25;
    let transform =
          Mat4::from_scale(Vec3::splat(scale))
        * Mat4::from_rotation_z(app.time * 0.33)
        * Mat4::from_rotation_x(app.mouse.y / 100.0)
        * Mat4::from_rotation_y(app.mouse.x / 100.0);
    tri.sort_by(|a, b| {
        let a = a.map_vertices(|(pt, _color)| pt);
        let b = b.map_vertices(|(pt, _color)| pt);
        let za = (transform * a.centroid().extend(1.0)).z;
        let zb = (transform * b.centroid().extend(1.0)).z;
        za.partial_cmp(&zb).unwrap_or(std::cmp::Ordering::Equal)
    });
    draw.transform(transform)
        .mesh()
        .tris_colored(tri);

    draw.to_frame(app, &frame).unwrap();
}
