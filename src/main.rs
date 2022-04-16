extern crate piralib;

use byte_slice_cast::AsByteSlice;
use byte_slice_cast::AsSliceOf;
use gst::BusSyncReply;
use gst::DebugGraphDetails;
use piralib::app;
use piralib::gl_helper as glh;
use piralib::glow;
use piralib::nalgebra_glm as glm;

use piralib::egui;
use piralib::event;
use piralib::utils::geo::Circle;
use piralib::utils::geo::Geometry;

use gst::prelude::*;
use gstreamer as gst;
use piralib::utils::geo::Rect;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use byte_slice_cast;
use image;

type VideoImage = image::ImageBuffer<image::Rgba<u8>, Vec<u8>>;

struct FrameData {
    shader: glh::GlslProg,
    vao: glh::Vao,

    // gstreamer
    main_loop: gst::glib::MainLoop,
    playbin: gst::Element,
    // bin: gst::Bin,
    texture: glh::Texture,
    image: Arc<Mutex<VideoImage>>,
}

fn m_setup(app: &mut app::App) -> FrameData {
    let gl = &app.gl;
    let attribs = Rect::new(0.0, 0.0, 720.0, 460.0)
        .texture_coords()
        .get_vertex_attribs(); //Circle::new(0.0, 0.0, 10.0).get_vertex_attribs(); //(0.0, 0.0, 10.0, false);
    let shader = glh::StockShader::new().texture(false).build(gl);
    let vao = glh::Vao::new_from_attrib(gl, &attribs, glow::TRIANGLE_FAN, &shader).unwrap();

    // ------ GSTREAMER ----------

    gst::init();
    let main_loop = gst::glib::MainLoop::new(None, false);

    let playbin = gst::ElementFactory::make("playbin", Some("playbinsink")).unwrap();

    let sink = gst::ElementFactory::make("appsink", Some("videosink")).unwrap();
    let app_sink = sink
        .dynamic_cast::<gst_app::AppSink>()
        .expect("Sink element should be an app sink");

    app_sink.set_wait_on_eos(true);
    app_sink.set_max_buffers(1);
    app_sink.set_drop(true);
    app_sink.set_qos(true);
    app_sink.set_sync(true);
    app_sink.set_max_lateness(20 * 1000);

    let texture = glh::Texture::new_from_data(
        gl,
        None,
        3840,
        2160,
        glh::texture::TextureSettings::default(),
    );

    let image = Arc::new(Mutex::new(image::RgbaImage::new(3840, 2160)));
    let image_clone = image.clone();

    app_sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos).unwrap();
                let buffer = sample.buffer().unwrap();
                let info: gst_video::VideoInfo =
                    gst_video::VideoInfo::from_caps(&sample.caps().unwrap()).unwrap();

                //println!("info: {} {}", info.width(), info.height());

                let mem_map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                let data = mem_map.as_slice_of::<u8>().unwrap();

                *image_clone.lock().unwrap() =
                    image::RgbaImage::from_raw(info.width(), info.height(), data.to_vec()).unwrap();
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    app_sink.set_caps(Some(
        &gst::Caps::builder("video/x-raw")
            .field("format", "RGBA")
            .build(),
    ));

    let video_bin = gst::Bin::new(Some("cinder-bin"));
    video_bin.add(&app_sink).unwrap();

    let pad = app_sink.static_pad(&"sink").unwrap(); //video_bin.static_pad(&"sink").unwrap();
    pad.set_active(true).unwrap();

    video_bin
        .add_pad(&gst::GhostPad::with_target(Some("sink"), &pad).unwrap())
        .unwrap();

    playbin.set_property("video-sink", &video_bin);

    playbin
        .bus()
        .unwrap()
        .set_sync_handler(|_, _| BusSyncReply::Pass);

    playbin
        .set_state(gst::State::Ready)
        .expect("Unable to set the pipeline to the `Playing` state");

    playbin.set_property(
        "uri",
        "file:///C:/Users/Henrique/Desktop/SpaceXLaunches4KDemo.mp4",
    );

    // playbin.set_property(
    //     "uri",
    //     "file:///C:/Users/Henrique/Documents/dev/rust/pira-gst/testsrc.mp4",
    // );

    playbin
        .set_state(gst::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    //gst::debug_bin_to_dot_file_with_ts(&playbin.into(), DebugGraphDetails::all(), "pira-gst");
    let main_loop_clone = main_loop.clone();

    playbin
        .bus()
        .unwrap()
        .add_watch(move |_, msg| {
            use gst::MessageView;
            let main_loop = &main_loop_clone;
            match msg.view() {
                MessageView::Eos(..) => {
                    println!("received eos");
                    // An EndOfStream event was sent to the pipeline, so we tell our main loop
                    // to stop execution here.
                    main_loop.quit()
                }
                MessageView::Error(err) => {
                    println!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    main_loop.quit();
                }
                _ => (),
            };

            gst::glib::Continue(true)
        })
        .ok();

    FrameData {
        vao,
        shader,
        main_loop,
        playbin,
        texture,
        image,
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
    let circle_shader = &data.shader;
    let circle_vao = &data.vao;
    let texture = &data.texture;

    let main_loop = &data.main_loop;
    main_loop.context().iteration(false);

    let lock = data.image.try_lock();

    if let Ok(ref mutex) = lock {
        texture.update(gl, mutex.as_byte_slice());
    }

    glh::clear(gl, 1.0, 0.0, 0.4, 1.0);

    circle_shader.bind(gl);
    let _s_tex = glh::ScopedBind::new(gl, texture);

    circle_shader.set_orthographic_matrix(
        gl,
        [
            app.input_state.window_size.0 as f32 * 2.0,
            app.input_state.window_size.1 as f32 * 2.0,
        ],
    );

    circle_shader.set_view_matrix(gl, &glm::Mat4::identity());

    let mut model_view = glm::Mat4::identity();
    model_view = glm::translate(&model_view, &glm::vec3(0.0, 0.0, 0.0));
    model_view = glm::scale(&model_view, &glm::vec3(4.0, 4.0, 4.0));

    circle_shader.set_uniform_mat4(
        gl,
        glh::StockShader::uniform_name_model_matrix(),
        &model_view,
    );
    circle_shader.set_uniform_4f(
        gl,
        glh::StockShader::uniform_name_color(),
        &[1.0, 1.0, 1.0, 1.0],
    );

    circle_vao.draw(gl);

    circle_shader.unbind(gl);
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
