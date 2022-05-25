use actix_web::{
    get, http::StatusCode, middleware, web, App, HttpResponse, HttpServer, Responder, ResponseError,
};
use bytes::Bytes;
use figment::{providers::Env, Figment};
use futures::TryStreamExt;
use serde::Deserialize;

mod bbc;
mod fetch;
mod hls;
mod s3_upload;
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

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct Config {
    pub base_url: String,
    pub listen_port: Option<u16>,
    pub s3_bucket: Option<String>,
    pub s3_base_url: Option<String>,
    pub s3_endpoint_url: Option<String>,
}

#[get("/ok")]
async fn ok() -> impl Responder {
    HttpResponse::Ok().body("ok")
}

#[get("/show/{pid}")]
async fn get_podcast_feed(
    config: web::Data<Config>,
    pid: web::Path<String>,
) -> Result<impl Responder, bbc::BbcResponseError> {
    let id = pid.into_inner();

    let response = sounds_proxy::get_podcast_feed(&config.base_url, &id).await?;

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "application/rss+xml"))
        .insert_header(("Cache-Control", "public, max-age=900"))
        .body(response))
}

#[get("/episode/{pid}.aac")]
async fn get_episode_aac(
    config: web::Data<Config>,
    pid: web::Path<String>,
) -> Result<impl Responder, bbc::BbcResponseError> {
    {
        let episode_id = pid.into_inner();

        if let Some(url) = sounds_proxy::get_episode_url(&episode_id).await? {
            // Public episode

            Ok(HttpResponse::PermanentRedirect()
                .insert_header((actix_web::http::header::LOCATION, url))
                .finish())
        } else {
            // Private episode, serve directly

            let stream = sounds_proxy::get_episode(&episode_id).await?;

            if let Some((s3_client, region)) =
                create_s3_client(&config.s3_bucket, &config.s3_endpoint_url).await
            {
                let bucket = config.s3_bucket.clone().unwrap();
                let stream = stream.map_ok(Bytes::from).map_err(|e| e.into());

                let s3_path = format!("{}.aac", episode_id);
                log::debug!("Uploading episode to s3://{}/{}", bucket, s3_path);

                s3_upload::try_put_async_stream(
                    &s3_client,
                    &bucket,
                    stream,
                    &s3_path,
                    Some("audio/aac"),
                )
                .await?;

                let url = match &config.s3_base_url {
                    Some(base_url) => format!("{}/{}.aac", base_url, episode_id),
                    None => format!(
                        "https://{}.s3.{}.amazonaws.com/{}.aac",
                        bucket, region, episode_id
                    ),
                };

                Ok(HttpResponse::TemporaryRedirect()
                    .insert_header((actix_web::http::header::LOCATION, url))
                    .finish())
            } else {
                let stream = stream.map_ok(|bytes| bytes.into());

                Ok(HttpResponse::Ok()
                    .content_type("audio/aac".to_string())
                    .insert_header(("Cache-Control", "public, max-age=604800"))
                    .streaming(stream))
            }
        }
    }
    .map_err(|e| {
        log::debug!("{}", e);
        e
    })
}

#[get("/episode/{pid}")]
async fn get_episode(
    config: web::Data<Config>,
    pid: web::Path<String>,
) -> Result<impl Responder, bbc::BbcResponseError> {
    let episode_id = pid.into_inner();

    if let Some(url) = sounds_proxy::get_episode_url(&episode_id).await? {
        // Public episode

        Ok(HttpResponse::PermanentRedirect()
            .insert_header((actix_web::http::header::LOCATION, url))
            .finish())
    } else {
        // Private episode, serve directly

        // At the moment only aac streams are supported
        Ok(HttpResponse::TemporaryRedirect()
            .insert_header((
                actix_web::http::header::LOCATION,
                format!("{}/episode/{}.aac", config.base_url, episode_id),
            ))
            .finish())
    }
}

async fn create_s3_client(
    bucket: &Option<String>,
    endpoint: &Option<String>,
) -> Option<(aws_sdk_s3::client::Client, String)> {
    if let Some(bucket) = bucket {
        let config_loader = aws_config::from_env();
        let config_loader = match endpoint {
            Some(endpoint) => {
                let url = endpoint.parse().unwrap();
                config_loader.endpoint_resolver(aws_sdk_s3::Endpoint::immutable(url))
            }
            None => config_loader,
        };
        let config = config_loader.load().await;
        let client = aws_sdk_s3::Client::new(&config);

        let region = client
            .get_bucket_location()
            .bucket(bucket)
            .send()
            .await
            .unwrap_or_else(|_| panic!("Failed to get bucket location for {}", bucket))
            .location_constraint
            .map_or_else(|| "us-east-1".to_string(), |region| region.as_str().into());

        Some((client, region))
    } else {
        None
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

    // create bucket to test config (will panic if bad)
    create_s3_client(&config.s3_bucket, &config.s3_endpoint_url).await;

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
            .wrap(middleware::Compress::default())
            .service(get_podcast_feed)
            .service(get_episode_aac)
            .service(get_episode)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
