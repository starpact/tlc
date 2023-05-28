mod handler;

use axum::{
    routing::{get, put},
    Router, Server,
};
use tokio::runtime::Builder;
use tower_http::trace::TraceLayer;
use tracing::info;

use handler::*;

fn main() -> anyhow::Result<()> {
    tlc_core::init();

    let app = Router::new()
        .route("/console", get(console))
        .nest(
            "/api",
            Router::new()
                .route("/name", get(get_name))
                .route("/name/:name", put(set_name))
                .route("/save_root_dir", get(get_save_root_dir))
                .route("/save_root_dir/:save_root_dir", put(set_save_root_dir))
                .route("/video_path", get(get_video_path))
                .route("/video_path/:video_path", put(set_video_path))
                .route("/video_nframes", get(get_video_nframes))
                .route("/video_frame_rate", get(get_video_frame_rate))
                .route("/video_shape", get(get_video_shape))
                .route("/decode_frame_base64", get(decode_frame_base64))
                .route("/daq_path", get(get_daq_path))
                .route("/daq_path/:daq_path", put(set_daq_path))
                .route("/daq_data", get(get_daq_data))
                .route("/synchronize_video_and_daq", put(synchronize_video_and_daq))
                .route("/start_frame", get(get_start_frame))
                .route("/start_frame/:start_frame", put(set_start_frame))
                .route("/start_row", get(get_start_row))
                .route("/start_row/:start_row", put(set_start_row))
                .route("/area", get(get_area).put(set_area))
                .route(
                    "/filter_method",
                    get(get_filter_method).put(set_filter_method),
                )
                .route("/filter_point", get(filter_point))
                .route(
                    "/thermocouples",
                    get(get_thermocouples).put(set_thermocouples),
                )
                .route(
                    "/interp_method",
                    get(get_interp_method).put(set_interp_method),
                )
                .route("/interp_frame/:frame_index", get(interp_frame))
                .route("/iter_method", get(get_iter_method).put(set_iter_method))
                .route(
                    "/physical_param",
                    get(get_physical_param).put(set_physical_param),
                )
                .route("/nu_data", get(get_nu_data))
                .route("/nu_plot", get(get_nu_plot))
                .route("/save_data", put(save_data))
                .with_state(Default::default()),
        )
        .layer(TraceLayer::new_for_http());

    let rt = Builder::new_current_thread().enable_all().build()?;
    let _guard = rt.enter();
    let server = Server::bind(&"127.0.0.1:6666".parse()?).serve(app.into_make_service());
    let console_url = format!("http://{}/console", server.local_addr());
    info!(console_url);

    // rt.spawn_blocking(move || webbrowser::open(&console_url).unwrap());
    rt.block_on(server)?;

    Ok(())
}
