use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, DATE};
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::Sha1;
use std::{collections::HashMap, path::Path};
use url::Url;
use uuid::Uuid;

type HmacSha1 = Hmac<Sha1>;

#[derive(Clone, Debug)]
pub struct MediaUploadInput {
    pub kind: String,
    pub url: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct UploadRoutePayload {
    pub url: String,
    pub file_id: String,
    pub filename: String,
    pub media_type: String,
    pub qwen_file: Value,
}

#[derive(Clone, Debug)]
struct DetectedType {
    mime: String,
    show_type: String,
    file_class: String,
    qwen_file_type: String,
}

#[derive(Debug, Deserialize)]
struct StsEnvelope {
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<StsData>,
}

#[derive(Clone, Debug, Deserialize)]
struct StsData {
    access_key_id: String,
    access_key_secret: String,
    security_token: String,
    file_url: String,
    file_path: String,
    file_id: String,
    bucketname: String,
    endpoint: String,
}

pub async fn upload_bytes_to_qwen(
    client: &reqwest::Client,
    qwen_base_url: &str,
    headers: &HashMap<String, String>,
    filename: String,
    content_type: Option<&str>,
    bytes: Vec<u8>,
) -> Result<UploadRoutePayload> {
    let detected = detect_file_type(&filename, content_type);
    validate_size(bytes.len(), &detected)?;
    let sts = get_sts_token(
        client,
        qwen_base_url,
        headers,
        &filename,
        bytes.len(),
        &detected.qwen_file_type,
    )
    .await?;
    let file_size = bytes.len();
    let file_url = upload_to_oss(client, &sts, &filename, &detected.mime, bytes).await?;
    Ok(UploadRoutePayload {
        url: file_url.clone(),
        file_id: sts.file_id.clone(),
        filename: filename.clone(),
        media_type: detected.qwen_file_type.clone(),
        qwen_file: build_qwen_file_entry(&sts.file_id, &file_url, &filename, detected, file_size),
    })
}

pub async fn prepare_multimodal_uploads(
    client: &reqwest::Client,
    qwen_base_url: &str,
    headers: &HashMap<String, String>,
    items: &[MediaUploadInput],
) -> Result<Vec<Value>> {
    let mut files = Vec::new();
    for item in items {
        let payload = if item.url.starts_with("http://") || item.url.starts_with("https://") {
            let response = client
                .get(&item.url)
                .send()
                .await
                .with_context(|| format!("failed to download {}", item.url))?;
            if !response.status().is_success() {
                return Err(anyhow!(
                    "failed to download {}: {}",
                    item.url,
                    response.status()
                ));
            }
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let bytes = response.bytes().await?.to_vec();
            let filename = filename_from_url(&item.url, &item.kind, content_type.as_deref());
            upload_bytes_to_qwen(
                client,
                qwen_base_url,
                headers,
                filename,
                content_type.as_deref(),
                bytes,
            )
            .await?
        } else if item.url.starts_with("data:") {
            let (content_type, bytes, filename) = decode_data_url(&item.url, &item.kind)?;
            upload_bytes_to_qwen(
                client,
                qwen_base_url,
                headers,
                filename,
                Some(&content_type),
                bytes,
            )
            .await?
        } else {
            return Err(anyhow!("unsupported multimodal URL scheme"));
        };
        files.push(payload.qwen_file);
    }
    Ok(files)
}

fn detect_file_type(filename: &str, content_type: Option<&str>) -> DetectedType {
    let mime = content_type
        .filter(|value| !value.is_empty() && *value != "application/octet-stream")
        .map(str::to_owned)
        .or_else(|| {
            mime_guess::from_path(filename)
                .first_raw()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "application/octet-stream".to_owned());

    if mime.starts_with("image/") {
        return DetectedType {
            mime,
            show_type: "image".to_owned(),
            file_class: "vision".to_owned(),
            qwen_file_type: "image".to_owned(),
        };
    }
    if mime.starts_with("video/") {
        return DetectedType {
            mime,
            show_type: "video".to_owned(),
            file_class: "video".to_owned(),
            qwen_file_type: "video".to_owned(),
        };
    }
    if mime.starts_with("audio/") {
        return DetectedType {
            mime,
            show_type: "audio".to_owned(),
            file_class: "audio".to_owned(),
            qwen_file_type: "audio".to_owned(),
        };
    }

    DetectedType {
        mime,
        show_type: "file".to_owned(),
        file_class: "file".to_owned(),
        qwen_file_type: "file".to_owned(),
    }
}

fn validate_size(size: usize, detected: &DetectedType) -> Result<()> {
    let max_size = match detected.qwen_file_type.as_str() {
        "video" => 100 * 1024 * 1024,
        "audio" => 50 * 1024 * 1024,
        _ => 20 * 1024 * 1024,
    };
    if size > max_size {
        return Err(anyhow!("file too large for {}", detected.qwen_file_type));
    }
    Ok(())
}

async fn get_sts_token(
    client: &reqwest::Client,
    qwen_base_url: &str,
    headers: &HashMap<String, String>,
    filename: &str,
    filesize: usize,
    filetype: &str,
) -> Result<StsData> {
    let response = client
        .post(format!("{qwen_base_url}/api/v2/files/getstsToken"))
        .header("Accept", "application/json, text/plain, */*")
        .header(CONTENT_TYPE, "application/json")
        .header("Cookie", headers.get("cookie").cloned().unwrap_or_default())
        .header("Origin", qwen_base_url)
        .header("Referer", format!("{qwen_base_url}/"))
        .header(
            "User-Agent",
            headers.get("user-agent").cloned().unwrap_or_default(),
        )
        .header("X-Request-Id", Uuid::new_v4().to_string())
        .header("bx-ua", headers.get("bx-ua").cloned().unwrap_or_default())
        .header(
            "bx-umidtoken",
            headers.get("bx-umidtoken").cloned().unwrap_or_default(),
        )
        .header("bx-v", headers.get("bx-v").cloned().unwrap_or_default())
        .json(&json!({
            "filename": filename,
            "filesize": filesize.to_string(),
            "filetype": filetype,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "STS token request failed: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let envelope: StsEnvelope = response.json().await?;
    if !envelope.success {
        return Err(anyhow!(
            "STS token invalid: {}",
            envelope
                .message
                .unwrap_or_else(|| "unknown upstream error".to_owned())
        ));
    }

    envelope
        .data
        .ok_or_else(|| anyhow!("STS token response missing data"))
}

async fn upload_to_oss(
    client: &reqwest::Client,
    sts: &StsData,
    _filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> Result<String> {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let canonical_headers = format!("x-oss-security-token:{}\n", sts.security_token);
    let canonical_resource = format!("/{}/{}", sts.bucketname, sts.file_path);
    let string_to_sign =
        format!("PUT\n\n{content_type}\n{date}\n{canonical_headers}{canonical_resource}");
    let mut mac = HmacSha1::new_from_slice(sts.access_key_secret.as_bytes())?;
    mac.update(string_to_sign.as_bytes());
    let signature = BASE64_STANDARD.encode(mac.finalize().into_bytes());

    let object_url = format!(
        "https://{}.{}/{}",
        sts.bucketname, sts.endpoint, sts.file_path
    );
    let response = client
        .put(&object_url)
        .header(CONTENT_TYPE, content_type)
        .header(DATE, date)
        .header(CONTENT_LENGTH, bytes.len())
        .header("x-oss-security-token", &sts.security_token)
        .header(
            "Authorization",
            format!("OSS {}:{signature}", sts.access_key_id),
        )
        .body(bytes)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "OSS upload failed: {} {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    Ok(sts
        .file_url
        .split('?')
        .next()
        .unwrap_or(sts.file_url.as_str())
        .to_owned())
}

fn build_qwen_file_entry(
    file_id: &str,
    file_url: &str,
    filename: &str,
    detected: DetectedType,
    file_size: usize,
) -> Value {
    json!({
        "type": detected.show_type,
        "file": {
            "created_at": proxy_core::current_timestamp(),
            "data": {},
            "filename": filename,
            "hash": Value::Null,
            "id": file_id,
            "user_id": "proxy-user",
            "meta": {
                "name": filename,
                "size": file_size,
                "content_type": detected.mime,
            },
            "update_at": proxy_core::current_timestamp(),
            "lastModified": proxy_core::current_timestamp(),
            "name": filename,
            "webkitRelativePath": "",
            "size": file_size,
            "type": detected.mime,
        },
        "id": file_id,
        "url": file_url,
        "name": filename,
        "collection_name": "",
        "progress": 100,
        "status": "uploaded",
        "greenNet": "success",
        "size": file_size,
        "error": "",
        "itemId": Uuid::new_v4().to_string(),
        "file_type": detected.mime,
        "showType": detected.show_type,
        "file_class": detected.file_class,
        "uploadTaskId": Uuid::new_v4().to_string(),
    })
}

fn decode_data_url(data_url: &str, kind: &str) -> Result<(String, Vec<u8>, String)> {
    let Some((metadata, payload)) = data_url.split_once(',') else {
        return Err(anyhow!("invalid data URL"));
    };
    let mime = metadata
        .strip_prefix("data:")
        .and_then(|rest| rest.split(';').next())
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream");
    let bytes = BASE64_STANDARD.decode(payload)?;
    let extension = mime_guess::get_mime_extensions_str(mime)
        .and_then(|extensions| extensions.first().copied())
        .unwrap_or("bin");
    let stem = match kind {
        "video_url" => "video",
        "audio_url" => "audio",
        "file_url" => "file",
        _ => "image",
    };
    Ok((
        mime.to_owned(),
        bytes,
        format!("{stem}_{}.{}", proxy_core::current_timestamp(), extension),
    ))
}

fn filename_from_url(url: &str, kind: &str, content_type: Option<&str>) -> String {
    if let Ok(parsed) = Url::parse(url) {
        if let Some(name) = Path::new(parsed.path())
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
        {
            if name.contains('.') {
                return name.to_owned();
            }
            let extension = content_type
                .and_then(|mime| mime_guess::get_mime_extensions_str(mime))
                .and_then(|extensions| extensions.first().copied())
                .unwrap_or("bin");
            return format!("{name}.{extension}");
        }
    }

    let stem = match kind {
        "video_url" => "video",
        "audio_url" => "audio",
        "file_url" => "file",
        _ => "image",
    };
    let extension = content_type
        .and_then(|mime| mime_guess::get_mime_extensions_str(mime))
        .and_then(|extensions| extensions.first().copied())
        .unwrap_or("bin");
    format!("{stem}_{}.{}", proxy_core::current_timestamp(), extension)
}
