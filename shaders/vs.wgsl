struct Data {
    world: mat4x4f,
    view: mat4x4f,
    proj: mat4x4f,
};

struct VertexOutput {
    @location(0) po: vec3f,
    @builtin(position) pos: vec4f,
};

@group(0) @binding(0)
var<uniform> uniforms: Data;

@vertex
fn main(@location(0) pos: vec3f) -> VertexOutput {
    let out_pos: vec4f = uniforms.proj * uniforms.view * uniforms.world * vec4f(pos, 1.0);
    let out_pos2: vec4f = vec4f(pos, 1.0);
    return VertexOutput(out_pos2.xyz, out_pos);
}
