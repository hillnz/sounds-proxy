use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
};

use crate::{bbc::QualityVariant, hls::HlsStream};

use super::bbc;

use chrono::DateTime;
use futures::{stream::Stream, StreamExt};
use itertools::*;
use regex::Regex;
use rss::{
    extension::itunes::{ITunesChannelExtensionBuilder, ITunesItemExtensionBuilder},
    ChannelBuilder, EnclosureBuilder, GuidBuilder, ImageBuilder, ItemBuilder,
};

type Result<T, E = bbc::BbcResponseError> = core::result::Result<T, E>;

fn template_url(url: String) -> Option<String> {
    let url_vars = HashMap::from([("recipe", "400x400")]);
    let re_url_vars = Regex::new(r"\{([^\{\}]+)\}").unwrap();

    let mut missing_vars = false;

    let url = re_url_vars.replace_all(&url, |caps: &regex::Captures| {
        let var = caps.get(1).unwrap().as_str();
        if !url_vars.contains_key(var) {
            missing_vars = true;
            log::warn!("Missing URL variable: {}", var);
            return "".into();
        }
        url_vars.get(var).unwrap().to_string()
    });

    if missing_vars {
        None
    } else {
        Some(url.into())
    }
}

pub async fn get_podcast_feed(base_url: &str, programme_id: &str) -> Result<String> {
    let urn = format!("urn:bbc:radio:series:{}", programme_id);

    let container = bbc::get_container(&urn).await?;

    let show_info = &container
        .data
        .iter()
        .find_map(|d| d.item())
        .ok_or(bbc::BbcResponseError::FormatError)?
        .data;

    log::debug!("{:?}", show_info);

    let image = show_info.image_url.clone().and_then(template_url);

    let subtitle = show_info
        .synopses
        .short
        .clone()
        .or_else(|| show_info.synopses.medium.clone())
        .or_else(|| show_info.synopses.long.clone());

    let rss_itunes = ITunesChannelExtensionBuilder::default()
        .author(Some(show_info.network.short_title.clone()))
        .block(Some("Yes".into()))
        .image(image.clone())
        .subtitle(subtitle)
        .build();

    let namespaces = BTreeMap::from([(
        "itunes".to_string(),
        "http://www.itunes.com/dtds/podcast-1.0.dtd".to_string(),
    )]);

    let mut most_recent_pubdate = None;

    let episodes = container
        .data
        .iter()
        .find_map(|d| d.list())
        .ok_or(bbc::BbcResponseError::FormatError)?
        .data
        .clone()
        .iter()
        .map(|d| {
            log::debug!("{:#?}", d);

            let variants = &d.download.quality_variants;
            let best_variant = variants
                .high
                .as_ref()
                .or(variants.medium.as_ref())
                .or(variants.low.as_ref());
            let url = best_variant
                .and_then(|v| v.file_url.clone())
                .unwrap_or_else(||
                    // No public url - we will proxy it instead
                    format!("{}/episode/{}", base_url, d.id));

            let file_size = match best_variant {
                Some(QualityVariant {
                    file_url: Some(_),
                    file_size: Some(s),
                }) => *s,
                _ => 50000 * d.duration.value, // estimate based on duration
            };

            let content_type = match best_variant {
                Some(QualityVariant {
                    file_url: Some(f), ..
                }) => match f.split('.').last() {
                    Some("mp3") => "audio/mpeg".to_string(),
                    Some("m4a") | Some("mp4") => "audio/mp4".to_string(),
                    _ => "audio/mpeg".to_string(),
                },
                _ => "audio/aac".to_string(),
            };

            let duration = format!(
                "{}:{:02}:{:02}",
                d.duration.value / 3600,
                (d.duration.value / 60) % 60,
                d.duration.value % 60
            );

            let guid = GuidBuilder::default().value(d.id.clone()).build();

            let pub_date = DateTime::parse_from_rfc3339(&d.release.date).ok();

            if most_recent_pubdate.is_none()
                || pub_date.is_some() && pub_date.unwrap() > most_recent_pubdate.unwrap()
            {
                most_recent_pubdate = pub_date;
            }

            let summary = d
                .synopses
                .long
                .clone()
                .or_else(|| d.synopses.medium.clone())
                .or_else(|| d.synopses.short.clone());

            let enclosure = EnclosureBuilder::default()
                .url(url)
                .length(file_size.to_string())
                .mime_type(content_type)
                .build();

            let image = d.image_url.clone().and_then(template_url);

            let it_item = ITunesItemExtensionBuilder::default()
                .duration(Some(duration))
                .author(Some(show_info.network.short_title.clone()))
                .subtitle(d.titles.secondary.clone())
                .summary(summary.clone())
                .image(image)
                .build();

            ItemBuilder::default()
                .title(d.titles.secondary.clone())
                .description(summary)
                .enclosure(Some(enclosure))
                .guid(Some(guid))
                .pub_date(pub_date.map(|d| d.to_rfc2822()))
                .itunes_ext(Some(it_item))
                .build()
        })
        .collect::<Vec<_>>();

    let image = image.map(|img| {
        ImageBuilder::default()
            .url(img)
            .width(Some("400".to_string()))
            .height(Some("400".to_string()))
            .build()
    });

    let mut rss_channel_builder = ChannelBuilder::default();
    rss_channel_builder
        .title(show_info.titles.primary.clone())
        .link("https://www.bbc.co.uk/sounds/series/".to_string() + programme_id)
        .itunes_ext(Some(rss_itunes))
        .namespaces(namespaces)
        .items(episodes)
        .pub_date(most_recent_pubdate.map(|d| d.to_rfc2822()))
        .image(image)
        .build();

    Ok(rss_channel_builder.build().to_string())
}

type TryBytes = Result<Vec<u8>>;

pub async fn get_episode_url(episode_id: &str) -> Result<Option<String>> {
    bbc::get_media_url(episode_id).await
}

pub async fn get_episode(episode_id: &str) -> Result<impl Stream<Item = TryBytes>> {
    let media = bbc::get_media(episode_id).await?;

    // locate highest quality audio
    let audio_url = media
        .media
        .iter()
        .filter(|m| m.kind == "audio")
        .sorted_by_key(|m| m.bitrate.parse::<u32>().unwrap_or(0))
        .last()
        .ok_or(bbc::BbcResponseError::NotFound)?
        .connection
        .iter()
        .sorted_by(|a, b| {
            if a.protocol == b.protocol {
                Ordering::Equal
            } else if a.protocol == "http" {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        })
        .last()
        .unwrap()
        .href
        .clone();

    if !audio_url.contains(".m3u8") {
        return Err(bbc::BbcResponseError::UnsupportedMedia(
            episode_id.into(),
            audio_url,
        ));
    }

    log::debug!("m3u8 url: {}", audio_url);

    let stream = HlsStream::new(audio_url)?.map(|r| r.map_err(|e| e.into()));

    Ok(stream)
}
