use poem::{listener::TcpListener, middleware::Tracing, EndpointExt, Route, Server};
use poem_openapi::OpenApiService;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use robot_control_server::api;
use robot_control_server::config::Settings;
use robot_control_server::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::from_env();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting robot control server on {}", settings.address());

    let state = AppState::new(settings.clone());

    let api_service = OpenApiService::new(
        (api::HealthApi, api::RobotsApi, api::CamerasApi),
        "Robot Control API",
        "0.1.0",
    );

    let ui = api_service.swagger_ui();

    let app = Route::new()
        .at("/api/robots/:serial_id/ws", api::robot_ws::robot_ws)
        .at("/api/cameras/ws", api::camera_ws::camera_ws)
        .nest("/", api_service)
        .nest("/docs", ui)
        .with(Tracing)
        .data(state);

    // let app = create_app();

    tracing::info!("Server listening on {}", settings.address());

    Server::new(TcpListener::bind(settings.address()))
        .run(app)
        .await?;

    Ok(())
}
