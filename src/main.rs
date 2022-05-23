extern crate piralib;

use byte_slice_cast::AsByteSlice;
use byte_slice_cast::AsSliceOf;
use gst::BusSyncReply;
use piralib::app;
use piralib::gl_helper as glh;
use piralib::glow;
use piralib::nalgebra_glm as glm;

use piralib::egui;
use piralib::event;
use piralib::utils::geo::Geometry;

use gst::prelude::*;
use gstreamer as gst;
use piralib::utils::geo::Rect;
use std::sync::{Arc, Mutex};

use byte_slice_cast;
use image;

type VideoImage = Option<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>>;

struct FrameData {
    shader: glh::GlslProg,
    vao: glh::Vao,
    texture: Option<glh::Texture>,
}

fn m_setup(app: &mut app::App) -> FrameData {
    let gl = &app.gl;
    let attribs = Rect::new(0.0, 0.0, 720.0, 460.0)
        .texture_coords()
        .get_vertex_attribs(); //Circle::new(0.0, 0.0, 10.0).get_vertex_attribs(); //(0.0, 0.0, 10.0, false);
    let shader = glh::StockShader::new().texture(false).build(gl);
    let vao = glh::Vao::new_from_attrib(gl, &attribs, glow::TRIANGLE_FAN, &shader).unwrap();

    FrameData {
        vao,
        shader,
        texture: None,
    }
}

fn m_event(_app: &mut app::App, _data: &mut FrameData, event: &event::WindowEvent) {
    if let event::WindowEvent::MouseInput { state, .. } = event {
        if matches!(state, event::ElementState::Pressed) {}
    }

    // if let event::WindowEvent::CursorMoved{ position, .. } = event {
    // }

    // if let event::WindowEvent::KeyboardInput { .. } = event {
    // }
}

fn m_update(app: &app::App, data: &mut FrameData, _ui: &egui::Context) {
    let gl = &app.gl;
    let quad_shader = &data.shader;
    let quad_vao = &data.vao;
    let texture = &mut data.texture;

    glh::clear(gl, 1.0, 0.0, 0.4, 1.0);
}

fn main() {
    let _ = gst::init();

    app::AppBuilder::new(
        app::AppSettings {
            window_size: (1920, 1080),
            window_title: "simple app",
        },
        m_setup,
    )
    .event(m_event)
    .run(m_update);
}
