use crate::bbc::BbcResponseError;

pub fn get_http_response_for_bbc_error(err: &BbcResponseError) -> (u16, Option<String>) {
    match err {
        BbcResponseError::NotFound => (404, None),
        BbcResponseError::FormatError => (503, Some("Unexpected data from BBC".into())),
        BbcResponseError::ServerResponseError(upstream_status) => {
            if *upstream_status == 400 {
                // 400 seems to be returned for a bad pid
                (404, None)
            } else {
                (
                    503,
                    Some(format!("Error response from BBC ({})", upstream_status)),
                )
            }
        }
        BbcResponseError::UnsupportedMedia(_, _) => {
            (501, Some("Media format not supported".into()))
        }
        _ => (500, None),
    }
}
