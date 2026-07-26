pub const MAX_HTTP_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_HTTP_CHUNK_HEADER_BYTES: usize = 8 * 1024;

pub fn decode_chunked_body(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut offset = 0_usize;

    loop {
        let remainder = input
            .get(offset..)
            .ok_or("invalid chunked replay response offset")?;
        let relative_line_end = remainder
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or("invalid chunked replay response")?;
        if relative_line_end > MAX_HTTP_CHUNK_HEADER_BYTES {
            return Err("HTTP chunk header exceeds 8 KiB".to_owned());
        }
        let line_end = offset
            .checked_add(relative_line_end)
            .ok_or("HTTP chunk header offset overflow")?;
        let size_text = std::str::from_utf8(&input[offset..line_end])
            .map_err(|_| "invalid chunk size")?
            .split(';')
            .next()
            .unwrap_or("");
        if size_text.is_empty() {
            return Err("invalid HTTP chunk size".to_owned());
        }
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| "invalid HTTP chunk size".to_owned())?;
        offset = line_end
            .checked_add(2)
            .ok_or("HTTP chunk offset overflow")?;

        if size == 0 {
            return validate_chunk_trailers(input, offset).map(|()| output);
        }

        let output_end = output
            .len()
            .checked_add(size)
            .ok_or("decoded HTTP body size overflow")?;
        if output_end > max_output_bytes {
            return Err(format!(
                "decoded HTTP body exceeds {} bytes",
                max_output_bytes
            ));
        }

        let end = offset.checked_add(size).ok_or("HTTP chunk size overflow")?;
        let frame_end = end
            .checked_add(2)
            .ok_or("HTTP chunk terminator offset overflow")?;
        if frame_end > input.len() || input.get(end..frame_end) != Some(b"\r\n") {
            return Err("truncated chunked replay response".to_owned());
        }
        output.extend_from_slice(&input[offset..end]);
        offset = frame_end;
    }
}

fn validate_chunk_trailers(input: &[u8], mut offset: usize) -> Result<(), String> {
    loop {
        let remainder = input
            .get(offset..)
            .ok_or("invalid chunked replay response trailer offset")?;
        let relative_line_end = remainder
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or("truncated chunked replay response trailer")?;
        if relative_line_end > MAX_HTTP_CHUNK_HEADER_BYTES {
            return Err("HTTP chunk trailer exceeds 8 KiB".to_owned());
        }
        if relative_line_end == 0 {
            return Ok(());
        }
        let line_end = offset
            .checked_add(relative_line_end)
            .ok_or("HTTP chunk trailer offset overflow")?;
        let trailer = input
            .get(offset..line_end)
            .ok_or("invalid HTTP chunk trailer")?;
        if !trailer.contains(&b':')
            || trailer
                .iter()
                .any(|byte| matches!(byte, b'\0' | b'\r' | b'\n'))
        {
            return Err("invalid HTTP chunk trailer".to_owned());
        }
        offset = line_end
            .checked_add(2)
            .ok_or("HTTP chunk trailer offset overflow")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_chunks_extensions_and_trailers() {
        let decoded = decode_chunked_body(
            b"4;name=value\r\nWiki\r\n5\r\npedia\r\n0\r\nX-Trace: yes\r\n\r\n",
            1024,
        )
        .unwrap();

        assert_eq!(decoded, b"Wikipedia");
    }

    #[test]
    fn rejects_overflowing_and_oversized_chunks_without_panicking() {
        assert!(decode_chunked_body(b"ffffffffffffffff\r\nx\r\n", 1024).is_err());
        assert!(decode_chunked_body(b"5\r\nhello\r\n0\r\n\r\n", 4).is_err());
        assert!(decode_chunked_body(b"1\r\nx\r\n0\r\n", 1024).is_err());
    }
}
