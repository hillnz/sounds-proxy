use actix_web::{
    get, http::StatusCode, middleware, web, App, HttpResponse, HttpServer, Responder, ResponseError,
};
use figment::{providers::Env, Figment};
use futures::TryStreamExt;
use serde::Deserialize;

mod bbc;
mod fetch;
mod hls;
mod s3;
mod sounds_proxy;
mod web_utils;

impl ResponseError for bbc::BbcResponseError {
    fn error_response(&self) -> HttpResponse {
        let (code, msg) = web_utils::get_http_response_for_bbc_error(self);
        let status = StatusCode::from_u16(code).unwrap();
        HttpResponse::build(status).body(msg.unwrap_or_else(|| "".into()))
    }

    fn status_code(&self) -> StatusCode {
        let (code, _) = web_utils::get_http_response_for_bbc_error(self);
        StatusCode::from_u16(code).unwrap()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Deserialize)]
struct Config {
    pub base_url: &'static str,
    pub listen_port: Option<u16>,
    pub s3_bucket: Option<&'static str>,
}

#[get("/show/{pid}")]
async fn get_podcast_feed(
    config: web::Data<Config>,
    pid: web::Path<String>,
) -> Result<impl Responder, bbc::BbcResponseError> {
    let id = pid.into_inner();

    let response = sounds_proxy::get_podcast_feed(config.base_url, &id).await?;

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "application/rss+xml"))
        .insert_header(("Cache-Control", "public, max-age=900"))
        .body(response))
}

#[get("/episode/{pid}")]
async fn get_episode(config: web::Data<Config>, pid: web::Path<String>) -> Result<impl Responder, bbc::BbcResponseError> {
    let episode_id = pid.into_inner();

    if let Some(url) = sounds_proxy::get_episode_url(&episode_id).await? {
        // Public episode

        Ok(HttpResponse::PermanentRedirect()
            .insert_header((actix_web::http::header::LOCATION, url))
            .finish())
    } else {
        // Private episode, serve directly

        let stream = sounds_proxy::get_episode(&episode_id).await?;

        

            .map_ok(|bytes| bytes.into());

        Ok(HttpResponse::Ok()
            .content_type("audio/aac".to_string())
            .insert_header(("Cache-Control", "public, max-age=604800"))
            .streaming(stream))
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let figment = Figment::new().merge(Env::prefixed("SOUNDS_PROXY_"));
    let config: Config = figment
        .extract()
        .map_err(|e| {
            println!("{}", e);
            println!("Set config fields by prefixing environment variables with 'SOUNDS_PROXY_'");
            e
        })
        .unwrap();
    let port = config.listen_port.unwrap_or(8080);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config))
            .wrap(middleware::Compress::default())
            .service(get_podcast_feed)
            .service(get_episode)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
