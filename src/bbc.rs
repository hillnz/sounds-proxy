use crate::hls::HlsError;

use super::fetch::{get, head, FetchError};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BbcResponseError {
    #[error("Not found")]
    NotFound,

    #[error("Server response code: {0}")]
    ServerResponseError(u16),

    #[error("BBC response not understood")]
    FormatError,

    #[error("Unsupported media: pid {0}, message {1}")]
    UnsupportedMedia(String, String),

    #[error("Unknown IO error")]
    IOError(#[from] std::io::Error),

    #[error("Fetch error: {0}")]
    FetchError(FetchError),

    #[error("HLS download error: {0}")]
    HlsDownloadError(#[from] HlsError),
}

impl From<FetchError> for BbcResponseError {
    fn from(err: FetchError) -> Self {
        if let FetchError::ResponseCode(code) = err {
            if code == 404 {
                BbcResponseError::NotFound
            } else {
                BbcResponseError::ServerResponseError(code)
            }
        } else {
            BbcResponseError::FetchError(err)
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Synopses {
    pub medium: Option<String>,
    pub long: Option<String>,
    pub short: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Titles {
    pub primary: String,
    pub secondary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Duration {
    pub value: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QualityVariant {
    pub file_url: Option<String>,
    pub file_size: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QualityVariants {
    pub low: Option<QualityVariant>,
    pub medium: Option<QualityVariant>,
    pub high: Option<QualityVariant>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Download {
    pub r#type: String,
    pub quality_variants: QualityVariants,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Network {
    pub short_title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContainerItemData {
    pub id: String,
    pub titles: Titles,
    pub synopses: Synopses,
    pub network: Network,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContainerListData {
    pub id: String,
    pub titles: Titles,
    pub synopses: Synopses,
    pub duration: Duration,
    pub release: Release,
    pub download: Download,
    pub image_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ContainerList {
    pub data: Vec<ContainerListData>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ContainerItem {
    pub data: ContainerItemData,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "id")]
pub enum Container {
    #[serde(alias = "container")]
    ContainerItem(ContainerItem),
    #[serde(alias = "container_list")]
    ContainerList(ContainerList),
}

impl Container {
    pub fn item(&self) -> Option<&ContainerItem> {
        match self {
            Container::ContainerItem(item) => Some(item),
            _ => None,
        }
    }

    pub fn list(&self) -> Option<&ContainerList> {
        match self {
            Container::ContainerList(list) => Some(list),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ContainerResponse {
    pub data: Vec<Container>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Connection {
    pub protocol: String,
    pub href: String,
    #[serde(alias = "transferFormat")]
    pub transfer_format: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Media {
    pub kind: String,
    pub r#type: String,
    pub bitrate: String,
    pub encoding: String,
    pub connection: Vec<Connection>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MediaList {
    pub media: Vec<Media>,
}

type Result<T, E = BbcResponseError> = std::result::Result<T, E>;

pub async fn get_container(urn: &str) -> Result<ContainerResponse> {
    let encoded_urn = utf8_percent_encode(urn, NON_ALPHANUMERIC).to_string();
    let uri = format!(
        "https://rms.api.bbc.co.uk/v2/experience/inline/container/{}",
        encoded_urn
    );

    let resp_text = get(uri).await?.text()?;

    let resp: ContainerResponse =
        serde_json::from_str(&resp_text).map_err(|_| BbcResponseError::FormatError)?;

    Ok(resp)
}

pub async fn get_media(pid: &str) -> Result<MediaList> {
    let encoded_pid = utf8_percent_encode(pid, NON_ALPHANUMERIC).to_string();
    let uri = format!("https://open.live.bbc.co.uk/mediaselector/6/select/version/2.0/format/json/mediaset/mobile-phone-main/vpid/{}/transferformat/hls/", 
        encoded_pid);

    let resp_text = get(uri).await?.text()?;

    let resp: MediaList =
        serde_json::from_str(&resp_text).map_err(|_| BbcResponseError::FormatError)?;

    Ok(resp)
}

pub async fn get_media_url(pid: &str) -> Result<Option<String>> {
    let media_url = format!("https://open.live.bbc.co.uk/mediaselector/6/redir/version/2.0/mediaset/audio-nondrm-download/proto/https/vpid/{}.mp3", pid);
    let resp = head(media_url.clone()).await?;

    if resp == 200 {
        Ok(Some(media_url))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_get_container() {
        let id = "urn:bbc:radio:series:p02pc9pj";

        let _eps = get_container(id).await.unwrap();

        println!("{:#?}", _eps);
    }

    #[tokio::test]
    async fn test_deserialise_example() {
        let example_path = "./payload_examples/container.json";
        let example_text = std::fs::read_to_string(example_path).unwrap();
        let _example: ContainerResponse = serde_json::from_str(&example_text).unwrap();

        println!("{:#?}", _example);
    }

    #[tokio::test]
    async fn test_deserialise_media() {
        let example_path = "./payload_examples/media.json";
        let example_text = std::fs::read_to_string(example_path).unwrap();
        let _example: MediaList = serde_json::from_str(&example_text).unwrap();

        println!("{:#?}", _example);
    }

    #[tokio::test]
    async fn test_get_media() {
        let id = "p0btf00q";

        let _media = get_media(id).await.unwrap();

        println!("{:#?}", _media);
    }
}
