use thiserror::Error;

#[derive(Error, Debug)]
pub enum FetchError {
    #[error("server response code: {0}")]
    ResponseCode(u16),

    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
}

pub struct Response {
    pub status: u16,
    bytes: Vec<u8>,
}

impl Response {
    pub fn status_error(&self) -> Result<(), FetchError> {
        if self.status < 400 {
            Ok(())
        } else {
            Err(FetchError::ResponseCode(self.status))
        }
    }

    pub fn text(&self) -> Result<String, FetchError> {
        self.status_error()?;
        Ok(String::from_utf8(self.bytes.clone()).unwrap())
    }
}

const USER_AGENT: &str =
    "BBCSounds/2.6.0.14059 (iPhone13,3; iOS 15.3.1) MediaSelectorClient/7.0.4 BBCHTTPClient/9.0.0";

pub async fn get(uri: String) -> Result<Response, FetchError> {
    let client = reqwest::Client::new();

    let resp = client
        .get(uri)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    Ok(Response {
        status: resp.status().as_u16(),
        bytes: resp.bytes().await.unwrap().to_vec(),
    })
}

pub async fn head(uri: String) -> Result<u16, FetchError> {
    let client = reqwest::Client::new();

    let resp = client
        .head(uri)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    Ok(resp.status().as_u16())
}
