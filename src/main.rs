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
}

fn model(app: &App) -> Model {
    let window_id = app.new_window().size(512, 512).view(view).build().unwrap();

    Model { window_id }
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
        (-1.0, -1.0, -1.0, -1.0),
        ( 1.0, -1.0, -1.0, -1.0),
        (-1.0,  1.0, -1.0, -1.0),
        ( 1.0,  1.0, -1.0, -1.0),
        // top
        (-1.0, -1.0,  1.0, -1.0),
        ( 1.0, -1.0,  1.0, -1.0),
        (-1.0,  1.0,  1.0, -1.0),
        ( 1.0,  1.0,  1.0, -1.0),
        // bottom
        (-1.0, -1.0, -1.0,  1.0),
        ( 1.0, -1.0, -1.0,  1.0),
        (-1.0,  1.0, -1.0,  1.0),
        ( 1.0,  1.0, -1.0,  1.0),
        // top
        (-1.0, -1.0,  1.0,  1.0),
        ( 1.0, -1.0,  1.0,  1.0),
        (-1.0,  1.0,  1.0,  1.0),
        ( 1.0,  1.0,  1.0,  1.0),
    ];
    /*
     * 2 6 (-1, 1)  3 7 (1,  1)
     *
     * 0 4 (-1,-1)  1 5 (1, -1)
     */

    // just a 3d to start with
    let quads = [
        (0, 1, 2, 3), // bot
        (4, 5, 6, 7), // top
        (0, 1, 4, 5), // front
        (2, 3, 6, 7), // back
        (2, 0, 6, 4), // left
        (1, 3, 5, 7), // right
    ];
    let v = |i: usize| Vec3::new(verts[i].0, verts[i].1, verts[i].2);
    let mut tri = quads.iter()
        .flat_map(|q| {
            let mid = 0.25 * (v(q.0) + v(q.1) + v(q.2) + v(q.3));
            let p = 0.5 * (mid + Vec3::splat(1.0));
            let c = lin_srgba(p.x, p.y, p.z, 0.9);
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
