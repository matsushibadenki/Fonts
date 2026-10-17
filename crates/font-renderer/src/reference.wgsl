@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct Input { @location(0) position:vec2<f32>, @location(1) uv:vec2<f32>, @location(2) opacity:f32 };
struct Output { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) opacity:f32 };
@vertex fn vs(v:Input)->Output { var o:Output; o.position=vec4<f32>(v.position,0.,1.); o.uv=v.uv; o.opacity=v.opacity; return o; }
@fragment fn fs(v:Output)->@location(0) vec4<f32> { let pixel=textureSample(image,image_sampler,v.uv); return vec4<f32>(pixel.rgb,pixel.a*v.opacity); }
