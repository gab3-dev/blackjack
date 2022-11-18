// Copyright (C) 2022 setzer22 and contributors
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{common, shader_manager::Shader};
use crate::prelude::r3;
use rend3::{
    graph::DataHandle,
    managers::TextureManager,
    util::bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
};
use rend3_routine::base::{BaseRenderGraph, BaseRenderGraphIntermediateState};
use wgpu::*;

pub enum DrawType<'a> {
    /// Uses vertex pulling with an index buffer. The vertex id is used to index
    /// the storage buffers.
    UseIndices {
        indices: &'a Buffer,
        num_indices: usize,
    },
    /// Uses vertex pulling without an index buffer, will draw instances of
    /// `num_vertices` and the instance id will is used to index the storage
    UseInstances {
        num_vertices: usize,
        num_instances: usize,
    },
}

/// Generic trait to set different parameters of the viewport display.
///
/// Will generate a layout with given storage buffers, textures and uniform
/// buffers. Any of the three could be left as empty and will be generated in
/// the following order: (storages, textures, uniforms). All bindings will be
/// added to bind group 1, since bind group 0 is already used by rend3.
pub trait RoutineLayout<
    const NUM_BUFFERS: usize = 0,
    const NUM_TEXTURES: usize = 0,
    const NUM_UNIFORMS: usize = 0,
>
{
    type Settings;

    /// Returns one wgpu buffer for each of the `NUM_BUFFERS` buffers
    fn get_wgpu_buffers(&self, settings: &Self::Settings) -> [&Buffer; NUM_BUFFERS];

    /// Returns one wgpu buffer for each of the `NUM_TEXTURES` buffers
    fn get_wgpu_textures<'a>(
        &'a self,
        texture_manager: &'a TextureManager,
        settings: &'a Self::Settings,
    ) -> [&'a TextureView; NUM_TEXTURES];

    /// Returns one wgpu uniform for eah of the `NUM_UNIFORMS` buffers
    fn get_wgpu_uniforms<'a>(&'a self, settings: &Self::Settings) -> [&Buffer; NUM_UNIFORMS];

    /// Returns the draw type that should be used to draw this routine. Either
    /// spawn a fixed number of primitives, or use an index buffer.
    fn get_draw_type(&self, settings: &Self::Settings) -> DrawType<'_>;

    fn num_buffers() -> usize {
        NUM_BUFFERS
    }

    fn num_textures() -> usize {
        NUM_TEXTURES
    }

    fn num_uniforms() -> usize {
        NUM_UNIFORMS
    }
}

pub struct Viewport3dRoutine<
    Layout: RoutineLayout<NUM_BUFFERS, NUM_TEXTURES, NUM_UNIFORMS>,
    const NUM_BUFFERS: usize = 0,
    const NUM_TEXTURES: usize = 0,
    const NUM_UNIFORMS: usize = 0,
> {
    name: String,
    bgl: BindGroupLayout,
    pipeline: RenderPipeline,
    pub buffers: Vec<Layout>,
}

impl<
        Layout: RoutineLayout<NUM_BUFFERS, NUM_TEXTURES, NUM_UNIFORMS> + 'static,
        const NUM_BUFFERS: usize,
        const NUM_TEXTURES: usize,
        const NUM_UNIFORMS: usize,
    > Viewport3dRoutine<Layout, NUM_BUFFERS, NUM_TEXTURES, NUM_UNIFORMS>
{
    pub fn new(
        name: &str,
        device: &Device,
        base: &BaseRenderGraph,
        shader: &Shader,
        topology: PrimitiveTopology,
        front_face: FrontFace,
    ) -> Self {
        let bgl = {
            let mut builder = BindGroupLayoutBuilder::new();
            for _ in 0..Layout::num_buffers() {
                builder.append(
                    ShaderStages::VERTEX_FRAGMENT,
                    BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    None,
                );
            }
            for _ in 0..Layout::num_textures() {
                builder.append(
                    ShaderStages::VERTEX_FRAGMENT,
                    BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    None,
                );
            }
            for _ in 0..Layout::num_uniforms() {
                builder.append(
                    ShaderStages::VERTEX_FRAGMENT,
                    BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    None,
                );
            }
            builder.build(device, Some(&format!("{name} bgl")))
        };

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&base.interfaces.forward_uniform_bgl, &bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(&format!("{name} render pipeline")),
            layout: Some(&pipeline_layout),
            vertex: shader.to_vertex_state(&[]),
            primitive: common::primitive_state(topology, front_face),
            depth_stencil: Some(common::depth_stencil(true)),
            multisample: MultisampleState::default(),
            fragment: Some(shader.get_fragment_state()),
            multiview: None,
        });

        Self {
            name: name.into(),
            pipeline,
            bgl,
            buffers: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        // Wgpu will deallocate resources when `Drop` is called for the buffers.
        self.buffers.clear()
    }

    fn create_bind_groups<'node>(
        &'node self,
        graph: &mut r3::RenderGraph<'node>,
        out_bgs: DataHandle<Vec<BindGroup>>,
        settings: &'node Layout::Settings,
    ) {
        let mut builder = graph.add_node(format!("{}: create bind groups", self.name));
        let pt_handle = builder.passthrough_ref(self);
        let out_bgs = builder.add_data_output(out_bgs);

        builder.build(
            move |pt, renderer, _encoder_or_pass, _temps, _ready, graph_data| {
                let this = pt.get(pt_handle);
                graph_data.set_data(
                    out_bgs,
                    Some(
                        self.buffers
                            .iter()
                            .map(|buffer| {
                                let mut builder = BindGroupBuilder::new();
                                for buffer in buffer.get_wgpu_buffers(settings) {
                                    builder.append_buffer(buffer);
                                }
                                for texture in buffer
                                    .get_wgpu_textures(graph_data.d2_texture_manager, settings)
                                {
                                    builder.append_texture_view(texture);
                                }
                                builder.build(&renderer.device, None, &this.bgl)
                            })
                            .collect(),
                    ),
                );
            },
        )
    }

    fn draw<'node>(
        &'node self,
        graph: &mut r3::RenderGraph<'node>,
        state: &BaseRenderGraphIntermediateState,
        in_bgs: DataHandle<Vec<BindGroup>>,
        settings: &'node Layout::Settings,
    ) {
        let mut builder = graph.add_node(format!("{}: draw", self.name));
        let color = builder.add_render_target_output(state.color);
        let depth = builder.add_render_target_output(state.depth);
        let in_bgs = builder.add_data_input(in_bgs);
        let resolve = builder.add_optional_render_target_output(state.resolve);
        let pt_handle = builder.passthrough_ref(self);
        let forward_uniform_bg = builder.add_data_input(state.forward_uniform_bg);

        let rpass_handle = builder.add_renderpass(r3::RenderPassTargets {
            targets: vec![r3::RenderPassTarget {
                color,
                clear: Color::BLACK,
                resolve,
            }],
            depth_stencil: Some(r3::RenderPassDepthTarget {
                target: r3::DepthHandle::RenderTarget(depth),
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        builder.build(
            move |pt, _renderer, encoder_or_pass, temps, _ready, graph_data| {
                let this = pt.get(pt_handle);
                let pass = encoder_or_pass.get_rpass(rpass_handle);

                let in_bgs = graph_data.get_data(temps, in_bgs).unwrap();
                let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_bg).unwrap();

                pass.set_pipeline(&this.pipeline);

                pass.set_bind_group(0, forward_uniform_bg, &[]);
                for (buffer, bg) in this.buffers.iter().zip(in_bgs.iter()) {
                    pass.set_bind_group(1, bg, &[]);

                    match buffer.get_draw_type(settings) {
                        DrawType::UseIndices {
                            indices,
                            num_indices,
                        } => {
                            pass.set_index_buffer(indices.slice(..), IndexFormat::Uint32);
                            pass.draw_indexed(0..num_indices as u32, 0, 0..1);
                        }
                        DrawType::UseInstances {
                            num_vertices,
                            num_instances,
                        } => {
                            pass.draw(0..num_vertices as u32, 0..num_instances as u32);
                        }
                    }
                }
            },
        );
    }

    pub fn add_to_graph<'node>(
        &'node self,
        graph: &mut r3::RenderGraph<'node>,
        state: &BaseRenderGraphIntermediateState,
        settings: &'node Layout::Settings,
    ) {
        let bgs = graph.add_data();
        self.create_bind_groups(graph, bgs, settings);
        self.draw(graph, state, bgs, settings);
    }
}
