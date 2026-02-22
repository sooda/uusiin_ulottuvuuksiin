struct PackedVec3 {
    x: f32,
    y: f32,
    z: f32,
};

struct Tris {
    vertices: array<PackedVec3, 144>,
    colors: array<PackedVec3, 48>,
};

struct Data {
    world: mat4x4f,
    iworld: mat4x4f,
    view: mat4x4f,
    proj: mat4x4f,
};

@group(0) @binding(0)
var<uniform> uniforms: Data;
@group(0) @binding(1)
var<storage, read> geometry: Tris;

fn tri(i: i32) -> vec3f {
    let v = geometry.vertices[i];
    return vec3f(v.x, v.y, v.z);
}

fn tric(i: i32) -> vec3f {
    let v = geometry.colors[i];
    return vec3f(v.x, v.y, v.z);
}

// Möller-Trumbore, from Tomas Akenine-Möller, MIT license
fn intersect_triangle1(orig: vec3f, dir: vec3f, v0: vec3f, v1: vec3f, v2: vec3f) -> vec4f {
    let eps: f32 = 0.000001;

    let edge1 = v1 - v0;
    let edge2 = v2 - v0;

    let pvec = cross(dir, edge2);
    let det = dot(edge1, pvec);

    var u: f32;
    var v: f32;
    var tvec: vec3f;
    var qvec: vec3f;

    if (det > eps) {
        tvec = orig - v0;
        u = dot(tvec, pvec);
        if (u < 0.0 || u > det) {
            return vec4f(0.0);
        }

        qvec = cross(tvec, edge1);
        v = dot(dir, qvec);
        if (v < 0.0 || u + v > det) {
            return vec4f(0.0);
        }
    } else if (det < -eps) {
        tvec = orig - v0;
        u = dot(tvec, pvec);
        if (u > 0.0 || u < det) {
            return vec4f(0.0);
        }

        qvec = cross(tvec, edge1);
        v = dot(dir, qvec);
        if (v > 0.0 || u + v < det) {
            return vec4f(0.0);
        }
    } else {
        return vec4f(0.0); // parallel
    }

    let inv_det = 1.0 / det;

    let t = dot(edge2, qvec) * inv_det;
    u = u * inv_det;
    v = v * inv_det;

    return vec4f(t, u, v, 1.0);
}

@fragment
fn main(@location(0) po: vec3f, @builtin(position) fragcoord: vec4f) -> @location(0) vec4f {
    let orig = (uniforms.iworld * vec4(0.3, 0.3, 2.5, 1.0)).xyz;
    let dir = normalize(po - orig);
    var color = vec3f(0.1, 0.0, 0.0);
    for (var i = 0; i < 2 * 24; i++) {
        let v0 = tri(3 * i + 0);
        let v1 = tri(3 * i + 1);
        let v2 = tri(3 * i + 2);
        let a = intersect_triangle1(orig, dir, v0, v1, v2);
        if a.w != 0.0 && a.x > 0.0 {
            color += vec3f(1.0f / 10.0f) * tric(i);
        }
    }
    let out_color = vec4f(color, 1.0);
    return out_color;
}
