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
use std::ptr::null;
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
    image: Arc<Mutex<glh::Texture>>,

    //GSTREAMER
    pipeline: gst::Pipeline,
    appsink: gst_app::AppSink,
    glupload: gst::Element,
    bus: gst::Bus,
}

fn create_pipeline() -> Result<(gst::Pipeline, gst_app::AppSink, gst::Element), Error> {
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

    let sink =
        gst::ElementFactory::make("glsinkbin", None).map_err(|_| MissingElement("glsinkbin"))?;

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

    let callbacks = gst_app::AppSinkCallbacks::builder()
        .new_sample(move |appsink| {
            let sample = appsink.pull_sample().unwrap();
            println!("Sample!");
            //let buffer = sample.buffer().unwrap();
            // let info: gst_video::VideoInfo =
            //     gst_video::VideoInfo::from_caps(&sample.caps().unwrap()).unwrap();

            // let mem = buffer.peek_memory(0);

            // unsafe {
            //     let mmm = mem.as_ptr() as *const gst_gl::GLMemoryRef;
            //     println!("gl-mem: {:?}", (*mmm).texture_id());

            //     image_clone.lock().unwrap().handle = Some(
            //         std::mem::transmute::<u32, glow::Texture>((*mmm).texture_id()),
            //     );
            //}

            Result::Ok(gst::FlowSuccess::Ok)
        })
        .build();

    //appsink.set_callbacks(callbacks);

    Ok((pipeline, appsink, glupload.unwrap()))
}

fn m_setup(app: &mut app::App) -> FrameData {
    use gst_gl::prelude::ContextGLExt;
    use gst_gl::prelude::GLContextExt;
    use piralib::glutin::platform::ContextTraitExt;

    let gl = &app.gl;
    let attribs = Rect::new(0.0, 0.0, 720.0, 460.0)
        .texture_coords()
        .get_vertex_attribs(); //Circle::new(0.0, 0.0, 10.0).get_vertex_attribs(); //(0.0, 0.0, 10.0, false);
    let shader = glh::StockShader::new().texture(false).build(gl);
    let vao = glh::Vao::new_from_attrib(gl, &attribs, glow::TRIANGLE_FAN, &shader).unwrap();

    // GSTREAMER ---------

    let (pipeline, appsink, glupload) = create_pipeline().unwrap();
    let bus = pipeline
        .bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    //let handle =
    // let gl_display = unsafe {
    //     let egl = app.context.get_egl_display().unwrap();
    //     gst_gl_egl::GLDisplayEGL::with_egl_display(egl as usize)
    //         .unwrap()
    //         .upcast::<gst_gl::GLDisplay>()
    // };

    let gl_display = gst_gl::GLDisplay::new();

    let mut shared_ctx_opt = None;

    let (prev_device, prev_hglrc) = unsafe {
        let prev_device = glutin_wgl_sys::wgl::GetCurrentDC();
        let prev_hglrc = glutin_wgl_sys::wgl::GetCurrentContext();

        (prev_device, prev_hglrc)
    };

    match unsafe { app.context.context().raw_handle() } {
        piralib::glutin::platform::windows::RawHandle::Wgl(wgl_handle) => {
            unsafe {
                let raw_handle = wgl_handle as libc::uintptr_t;
                glutin_wgl_sys::wgl::MakeCurrent(null(), null());

                shared_ctx_opt = gst_gl::GLContext::new_wrapped(
                    &gl_display,
                    raw_handle,
                    gst_gl::GLPlatform::WGL,
                    gst_gl::GLAPI::OPENGL,
                );

                glutin_wgl_sys::wgl::MakeCurrent(prev_device, prev_hglrc);
            };
        }
        _ => (),
    };

    unsafe {}

    let shared_ctx = shared_ctx_opt.unwrap();

    shared_ctx
        .activate(true)
        .expect("Couldn't activate wrapped GL context");

    shared_ctx.fill_info().unwrap();
    let gl_context = shared_ctx.clone();

    #[allow(clippy::single_match)]
    bus.set_sync_handler(move |_, msg| {
        match msg.view() {
            gst::MessageView::NeedContext(ctxt) => {
                let context_type = ctxt.context_type();
                if context_type == *gst_gl::GL_DISPLAY_CONTEXT_TYPE {
                    if let Some(el) = msg.src().map(|s| s.downcast::<gst::Element>().unwrap()) {
                        let context = gst::Context::new(context_type, true);
                        context.set_gl_display(&gl_display);
                        el.set_context(&context);
                    }
                }
                if context_type == "gst.gl.app_context" {
                    if let Some(el) = msg.src().map(|s| s.downcast::<gst::Element>().unwrap()) {
                        let mut context = gst::Context::new(context_type, true);
                        {
                            let context = context.get_mut().unwrap();
                            let s = context.structure_mut();
                            s.set("context", &gl_context);
                        }
                        el.set_context(&context);
                    }
                }
            }
            _ => (),
        }

        gst::BusSyncReply::Pass
    });

    let image = Arc::new(Mutex::new(glh::Texture {
        handle: None,
        width: 320,
        height: 240,
        settings: glh::texture::TextureSettings::default(),
    }));
    let image_clone = image.clone();

    pipeline.set_state(gst::State::Playing).unwrap();

    FrameData {
        vao,
        shader,
        texture: None,
        image,

        pipeline,
        appsink,
        glupload,
        bus,
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
