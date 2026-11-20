use bytes::Bytes;
use reqwest::{header, Response, StatusCode};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::Mutex,
};

use super::{checked, limited, Result, ServerSession, StreamLease, API_TIMEOUT};

const BLOCK: u64 = 256 * 1024;

enum Transport {
    Ranged { length: u64, etag: String },
    Sequential { response: Mutex<Response> },
}

/// 解码线程专用。可定位表示按需读取有界字节块，转码或在线顺序流只保留一个网络块。
pub struct StreamReader {
    session: ServerSession,
    lease: StreamLease,
    transport: Transport,
    position: u64,
    buffer: Bytes,
    start: u64,
}

fn header_text<'a>(response: &'a Response, name: header::HeaderName) -> Option<&'a str> {
    response.headers().get(name)?.to_str().ok()
}

fn check_mime(response: &Response, mime: &str) -> Result<()> {
    if header_text(response, header::CONTENT_TYPE)
        .and_then(|s| s.split(';').next())
        .is_some_and(|s| s.trim().eq_ignore_ascii_case(mime))
    {
        Ok(())
    } else {
        Err("MusicFS stream did not match its negotiated audio format.".into())
    }
}

impl ServerSession {
    pub fn open_audio_reader(&self, lease: &StreamLease) -> Result<StreamReader> {
        self.check_stream(lease)?;
        let transport = self.runtime.block_on(async {
            if lease.byte_range {
                let response = self
                    .client
                    .head(lease.url.clone())
                    .timeout(API_TIMEOUT)
                    .send()
                    .await
                    .map_err(|_| "Unable to inspect the MusicFS audio stream.")?;
                let response = checked(response).await?;
                check_mime(&response, &lease.mime)?;
                let length = header_text(&response, header::CONTENT_LENGTH)
                    .and_then(|s| s.parse().ok())
                    .ok_or("Missing audio stream length.")?;
                let etag = header_text(&response, header::ETAG)
                    .filter(|s| s.starts_with('"') && s.ends_with('"'))
                    .ok_or("Missing audio stream identity.")?
                    .to_owned();
                Ok::<_, String>(Transport::Ranged { length, etag })
            } else {
                let response = self
                    .client
                    .get(lease.url.clone())
                    .send()
                    .await
                    .map_err(|_| "Unable to open MusicFS audio stream.")?;
                let response = checked(response).await?;
                check_mime(&response, &lease.mime)?;
                Ok(Transport::Sequential {
                    response: Mutex::new(response),
                })
            }
        })?;
        self.check_stream(lease)?;
        Ok(StreamReader {
            session: self.clone(),
            lease: lease.clone(),
            transport,
            position: 0,
            start: 0,
            buffer: Bytes::new(),
        })
    }
}

impl StreamReader {
    pub fn byte_length(&self) -> Option<u64> {
        match &self.transport {
            Transport::Ranged { length, .. } => Some(*length),
            Transport::Sequential { .. } => None,
        }
    }
    pub fn byte_seekable(&self) -> bool {
        self.byte_length().is_some()
    }

    fn refill(&mut self) -> Result<()> {
        let start = self.position;
        self.start = start;
        self.buffer = match &mut self.transport {
            Transport::Ranged { length, etag } => {
                if start >= *length {
                    return Ok(());
                }
                let end = start.saturating_add(BLOCK - 1).min(*length - 1);
                let expected = format!("bytes {start}-{end}/{length}");
                let etag: &str = etag;
                self.session.runtime.block_on(async {
                    let response = self
                        .session
                        .client
                        .get(self.lease.url.clone())
                        .header(header::RANGE, format!("bytes={start}-{end}"))
                        .header(header::IF_RANGE, etag)
                        .timeout(API_TIMEOUT)
                        .send()
                        .await
                        .map_err(|_| "Unable to read the requested audio range.")?;
                    if response.status() != StatusCode::PARTIAL_CONTENT
                        || header_text(&response, header::ETAG) != Some(etag)
                        || header_text(&response, header::CONTENT_RANGE) != Some(expected.as_str())
                    {
                        return Err("MusicFS audio range or representation changed.".into());
                    }
                    check_mime(&response, &self.lease.mime)?;
                    let count = (end - start + 1) as usize;
                    let bytes = limited(response, count).await?;
                    if bytes.len() != count {
                        return Err("Incomplete MusicFS audio range.".into());
                    }
                    Ok::<Bytes, String>(bytes.into())
                })?
            }
            Transport::Sequential { response } => {
                let response = response
                    .get_mut()
                    .map_err(|_| "MusicFS stream interrupted.")?;
                self.session
                    .runtime
                    .block_on(response.chunk())
                    .map_err(|_| "MusicFS audio stream interrupted.")?
                    .unwrap_or_default()
            }
        };
        self.session.check_stream(&self.lease)
    }
}

impl Read for StreamReader {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.session
            .check_stream(&self.lease)
            .map_err(io::Error::other)?;
        if dst.is_empty() || self.byte_length().is_some_and(|len| self.position >= len) {
            return Ok(0);
        }
        if self.position < self.start || self.position - self.start >= self.buffer.len() as u64 {
            self.refill().map_err(io::Error::other)?;
        }
        let offset = (self.position - self.start) as usize;
        let count = dst.len().min(self.buffer.len() - offset);
        dst[..count].copy_from_slice(&self.buffer[offset..offset + count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for StreamReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        if pos == SeekFrom::Current(0) {
            return Ok(self.position);
        }
        let length = self.byte_length().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Unsupported, "MusicFS stream is sequential.")
        })?;
        let target = match pos {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::Current(n) => self.position.checked_add_signed(n),
            SeekFrom::End(n) => length.checked_add_signed(n),
        }
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid audio stream position.",
            )
        })?;
        self.position = target;
        Ok(target)
    }
}
