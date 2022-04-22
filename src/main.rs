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

    // gstreamer
    main_loop: gst::glib::MainLoop,
    _playbin: gst::Element,
    // bin: gst::Bin,
    texture: Option<glh::Texture>,
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
    gst::init().unwrap();
    let main_loop = gst::glib::MainLoop::new(None, false);
    let playbin = gst::ElementFactory::make("playbin", Some("playbinsink")).unwrap();

    let playbin_d = playbin.downgrade();

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

    let image = Arc::new(Mutex::new(None));
    let image_preroll = image.clone();
    let image_clone = image.clone();

    app_sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_preroll(move |sink| {
                let sample = sink.pull_preroll().unwrap();
                let info = gst_video::VideoInfo::from_caps(&sample.caps().unwrap()).unwrap();

                println!("------- PREROLL -------");
                let image = &mut *image_preroll.lock().unwrap();

                if image.is_none() {
                    *image = Some(image::RgbaImage::new(info.width(), info.height()));
                }

                Ok(gst::FlowSuccess::Ok)
            })
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos).unwrap();
                let buffer = sample.buffer().unwrap();
                let info: gst_video::VideoInfo =
                    gst_video::VideoInfo::from_caps(&sample.caps().unwrap()).unwrap();

                let mem_map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                let data = mem_map.as_slice_of::<u8>().unwrap();

                *image_clone.lock().unwrap() = Some(
                    image::RgbaImage::from_raw(info.width(), info.height(), data.to_vec()).unwrap(),
                );
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    app_sink.set_caps(Some(
        &gst::Caps::builder("video/x-raw")
            .field("format", "RGBA")
            .build(),
    ));

    playbin
        .bus()
        .unwrap()
        .add_watch(move |_bus, msg| {
            use gst::MessageView;
            match msg.view() {
                MessageView::Eos(..) => {
                    println!("------EOS-----");
                    let playbin = playbin_d.upgrade().unwrap();
                    playbin
                        .seek_simple(gst::SeekFlags::FLUSH, 0 * gst::ClockTime::SECOND)
                        .unwrap();
                    playbin.set_state(gst::State::Playing).unwrap();
                }
                MessageView::Error(err) => {
                    println!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    gst::glib::Continue(false);
                }
                _ => (),
            };
            gst::glib::Continue(true)
        })
        .ok();

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

    FrameData {
        vao,
        shader,
        _playbin: playbin,
        main_loop,
        texture: None,
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
    let quad_shader = &data.shader;
    let quad_vao = &data.vao;
    let texture = &mut data.texture;
    let lock = data.image.try_lock();

    let main_loop = &data.main_loop;
    main_loop.context().iteration(false);

    if let Ok(ref mutex) = lock {
        if let Some(data) = mutex.as_ref() {
            if let Some(texture) = texture {
                texture.update(gl, data.as_byte_slice());
            } else {
                *texture = Some(glh::Texture::new_from_data(
                    gl,
                    None,
                    data.width().try_into().unwrap(),
                    data.height().try_into().unwrap(),
                    glh::texture::TextureSettings::default(),
                ));
            }
        }
    }

    glh::clear(gl, 1.0, 0.0, 0.4, 1.0);

    if let Some(texture) = texture {
        quad_shader.bind(gl);
        let _s_tex = glh::ScopedBind::new(gl, texture);

        quad_shader.set_orthographic_matrix(
            gl,
            [
                app.input_state.window_size.0 as f32 * 2.0,
                app.input_state.window_size.1 as f32 * 2.0,
            ],
        );

        quad_shader.set_view_matrix(gl, &glm::Mat4::identity());

        let mut model_view = glm::Mat4::identity();
        model_view = glm::translate(&model_view, &glm::vec3(0.0, 0.0, 0.0));
        model_view = glm::scale(&model_view, &glm::vec3(4.0, 4.0, 4.0));

        quad_shader.set_uniform_mat4(
            gl,
            glh::StockShader::uniform_name_model_matrix(),
            &model_view,
        );
        quad_shader.set_uniform_4f(
            gl,
            glh::StockShader::uniform_name_color(),
            &[1.0, 1.0, 1.0, 1.0],
        );

        quad_vao.draw(gl);
        quad_shader.unbind(gl);
    }
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
