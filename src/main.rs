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

use anyhow::Error;
use derive_more::{Display, Error};

#[derive(Debug, Display, Error)]
#[display(fmt = "Missing element {}", _0)]
struct MissingElement(#[error(not(source))] &'static str);

type VideoImage = Option<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>>;

struct FrameData {
    shader: glh::GlslProg,
    vao: glh::Vao,
    texture: Option<glh::Texture>,

    //GSTREAMER
    pipeline: gst::Pipeline,
    appsink: gst_app::AppSink,
    glupload: gst::Element,
    bus: gst::Bus,
}

fn create_pipeline() -> Result<(), Error> {
    let pipeline = gst::Pipeline::new(None);
    let src = gst::ElementFactory::make("videotestsrc", None)
        .map_err(|_| MissingElement("videotestsrc"))?;

    let appsink = gst::ElementFactory::make("appsink", None)
        .map_err(|_| MissingElement("appsink"))?
        .dynamic_cast::<gst_app::AppSink>()
        .expect("Sink element is expected to be an appsink!");

    appsink.set_property("enable-last-sample", false);
    appsink.set_property("emit-signals", false);
    appsink.set_property("max-buffers", 1u32);

    let caps = gst::Caps::builder("video/x-raw")
        .features(&[&gst_gl::CAPS_FEATURE_MEMORY_GL_MEMORY])
        .field("format", gst_video::VideoFormat::Rgba.to_str())
        .field("texture-target", "2D")
        .build();
    appsink.set_caps(Some(&caps));

    if let Some(gl_element) = gl_element {
        let glupload =
            gst::ElementFactory::make("glupload", None).map_err(|_| MissingElement("glupload"))?;

        pipeline.add_many(&[&src, &glupload])?;
        pipeline.add(gl_element)?;
        pipeline.add(&appsink)?;

        src.link(&glupload)?;
        glupload.link(gl_element)?;
        gl_element.link(&appsink)?;

        Ok((pipeline, appsink, glupload))
    } else {
        let sink = gst::ElementFactory::make("glsinkbin", None)
            .map_err(|_| MissingElement("glsinkbin"))?;

        sink.set_property("sink", &appsink);

        pipeline.add_many(&[&src, &sink])?;
        src.link(&sink)?;

        // get the glupload element to extract later the used context in it
        let mut iter = sink.dynamic_cast::<gst::Bin>().unwrap().iterate_elements();
        let glupload = loop {
            match iter.next() {
                Ok(Some(element)) => {
                    if "glupload" == element.factory().unwrap().name() {
                        break Some(element);
                    }
                }
                Err(gst::IteratorError::Resync) => iter.resync(),
                _ => break None,
            }
        };

        Ok((pipeline, appsink, glupload.unwrap()))
    }
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
