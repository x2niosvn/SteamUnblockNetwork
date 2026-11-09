/// Thông tin trích xuất được từ gói tin TLS ClientHello đầu tiên
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SniInfo {
    /// Tên miền SNI (ví dụ: "store.steampowered.com")
    pub hostname: String,
    /// Vị trí byte bắt đầu của chuỗi hostname trong mảng raw packet
    pub sni_start_offset: usize,
    /// Độ dài byte của chuỗi hostname
    pub sni_length: usize,
    /// Độ dài của TLS Record đầu tiên (5 byte header + payload_len)
    pub record_total_len: usize,
}

/// Phân tích gói tin TLS ClientHello để lấy tên miền SNI và vị trí byte chính xác
pub fn parse_tls_client_hello_sni(buf: &[u8]) -> Option<SniInfo> {
    if buf.len() < 43 || buf[0] != 0x16 || buf[1] != 0x03 {
        return None;
    }

    let record_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
    let record_total_len = 5 + record_len;
    let limit = buf.len().min(record_total_len);

    if limit < 44 || buf[5] != 0x01 {
        return None;
    }

    let mut pos = 43;

    // 1. Session ID
    if pos >= limit {
        return None;
    }
    let session_id_len = buf[pos] as usize;
    pos += 1 + session_id_len;

    // 2. Cipher Suites
    if pos + 2 > limit {
        return None;
    }
    let cipher_suites_len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
    pos += 2 + cipher_suites_len;

    // 3. Compression Methods
    if pos >= limit {
        return None;
    }
    let compression_len = buf[pos] as usize;
    pos += 1 + compression_len;

    // 4. Extensions
    if pos + 2 > limit {
        return None;
    }
    let extensions_len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
    pos += 2;
    let extensions_end = (pos + extensions_len).min(limit);

    while pos + 4 <= extensions_end {
        let ext_type = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let ext_len = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]) as usize;
        pos += 4;

        if pos + ext_len > extensions_end {
            break;
        }

        // Extension 0x0000 = server_name (SNI)
        if ext_type == 0x0000 && ext_len >= 5 {
            let sni_list_len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
            let mut sni_pos = pos + 2;
            let sni_end = (sni_pos + sni_list_len).min(pos + ext_len);

            while sni_pos + 3 <= sni_end {
                let name_type = buf[sni_pos];
                let name_len = u16::from_be_bytes([buf[sni_pos + 1], buf[sni_pos + 2]]) as usize;
                sni_pos += 3;

                if sni_pos + name_len > sni_end {
                    break;
                }

                if name_type == 0x00 && name_len > 0 {
                    if let Ok(hostname) = std::str::from_utf8(&buf[sni_pos..sni_pos + name_len]) {
                        return Some(SniInfo {
                            hostname: hostname.to_lowercase(),
                            sni_start_offset: sni_pos,
                            sni_length: name_len,
                            record_total_len,
                        });
                    }
                }
                sni_pos += name_len;
            }
        }

        pos += ext_len;
    }

    None
}

/// Chia nhỏ gói tin TLS ClientHello thành các TLS Handshake Records hợp chuẩn RFC:
/// - Mảnh 1: 2 byte đầu của Handshake Header (vô hiệu hóa bộ nhận diện ClientHello của tường lửa DPI)
/// - Mảnh 2: Từ byte thứ 2 đến đúng giữa chuỗi tên miền SNI
/// - Mảnh 3: Nửa sau của tên miền SNI + phần còn lại của ClientHello
pub fn build_fragmented_tls_records(buf: &[u8], sni: &SniInfo) -> Vec<Vec<u8>> {
    if buf.len() < sni.record_total_len || sni.sni_start_offset < 8 || sni.sni_length < 2 {
        let mid = buf.len() / 2;
        return vec![buf[..mid].to_vec(), buf[mid..].to_vec()];
    }

    let ver_major = buf[1];
    let ver_minor = buf[2];
    let payload = &buf[5..sni.record_total_len];

    let rel_sni_start = sni.sni_start_offset - 5;
    let rel_sni_mid = rel_sni_start + (sni.sni_length / 2);

    if rel_sni_mid <= 2 || rel_sni_mid >= payload.len() {
        let mid = buf.len() / 2;
        return vec![buf[..mid].to_vec(), buf[mid..].to_vec()];
    }

    let mut chunks = Vec::with_capacity(3);

    // Mảnh TLS Record 1: 2 byte đầu của Handshake (0x01, 0x00)
    let part1 = &payload[..2];
    let mut rec1 = Vec::with_capacity(5 + part1.len());
    rec1.extend_from_slice(&[
        0x16,
        ver_major,
        ver_minor,
        (part1.len() >> 8) as u8,
        (part1.len() & 0xFF) as u8,
    ]);
    rec1.extend_from_slice(part1);
    chunks.push(rec1);

    // Mảnh TLS Record 2: Từ byte 2 đến đúng giữa tên miền SNI
    let part2 = &payload[2..rel_sni_mid];
    let mut rec2 = Vec::with_capacity(5 + part2.len());
    rec2.extend_from_slice(&[
        0x16,
        ver_major,
        ver_minor,
        (part2.len() >> 8) as u8,
        (part2.len() & 0xFF) as u8,
    ]);
    rec2.extend_from_slice(part2);
    chunks.push(rec2);

    // Mảnh TLS Record 3: Nửa sau tên miền SNI + phần còn lại của ClientHello
    let part3 = &payload[rel_sni_mid..];
    let mut rec3 = Vec::with_capacity(5 + part3.len() + (buf.len() - sni.record_total_len));
    rec3.extend_from_slice(&[
        0x16,
        ver_major,
        ver_minor,
        (part3.len() >> 8) as u8,
        (part3.len() & 0xFF) as u8,
    ]);
    rec3.extend_from_slice(part3);
    if buf.len() > sni.record_total_len {
        rec3.extend_from_slice(&buf[sni.record_total_len..]);
    }
    chunks.push(rec3);

    chunks
}

/// Trích xuất Host header từ HTTP request (cổng 80) và vị trí để chia nhỏ gói tin HTTP
pub fn parse_http_host(buf: &[u8]) -> Option<(String, usize)> {
    let text = std::str::from_utf8(buf).ok()?;
    for line in text.lines() {
        if line.len() > 5 && line[..5].eq_ignore_ascii_case("host:") {
            let raw_val = &line[5..];
            let trimmed = raw_val.trim();
            let host = trimmed.split(':').next()?.trim().to_lowercase();
            if !host.is_empty() {
                if let Some(idx) = text.find(trimmed) {
                    let split_pos = idx + (trimmed.len() / 2).max(1);
                    return Some((host, split_pos));
                }
                return Some((host, buf.len() / 2));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_fragment_sni() {
        let hostname = b"store.steampowered.com";
        let mut ext_sni = Vec::new();
        let list_len = (3 + hostname.len()) as u16;
        ext_sni.extend_from_slice(&list_len.to_be_bytes());
        ext_sni.push(0x00);
        ext_sni.extend_from_slice(&(hostname.len() as u16).to_be_bytes());
        ext_sni.extend_from_slice(hostname);

        let mut extensions = Vec::new();
        extensions.extend_from_slice(&0x0000u16.to_be_bytes());
        extensions.extend_from_slice(&(ext_sni.len() as u16).to_be_bytes());
        extensions.extend_from_slice(&ext_sni);

        extensions.extend_from_slice(&0x0017u16.to_be_bytes());
        extensions.extend_from_slice(&0u16.to_be_bytes());

        let mut ch_body = Vec::new();
        ch_body.extend_from_slice(&[0x03, 0x03]);
        ch_body.extend_from_slice(&[0xAA; 32]);
        ch_body.push(0);
        ch_body.extend_from_slice(&2u16.to_be_bytes());
        ch_body.extend_from_slice(&[0x13, 0x01]);
        ch_body.push(1);
        ch_body.push(0);
        ch_body.extend_from_slice(&(extensions.len() as u16).to_be_bytes());
        ch_body.extend_from_slice(&extensions);

        let mut handshake = Vec::new();
        handshake.push(0x01);
        let hs_len = ch_body.len() as u32;
        handshake.push(((hs_len >> 16) & 0xFF) as u8);
        handshake.push(((hs_len >> 8) & 0xFF) as u8);
        handshake.push((hs_len & 0xFF) as u8);
        handshake.extend_from_slice(&ch_body);

        let mut record = Vec::new();
        record.extend_from_slice(&[0x16, 0x03, 0x01]);
        record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
        record.extend_from_slice(&handshake);

        let info = parse_tls_client_hello_sni(&record).expect("Should parse SNI");
        assert_eq!(info.hostname, "store.steampowered.com");

        let chunks = build_fragmented_tls_records(&record, &info);
        assert_eq!(chunks.len(), 3);

        for chunk in &chunks {
            let text = String::from_utf8_lossy(chunk);
            assert!(!text.contains("store.steampowered.com"));
        }

        let mut reassembled = Vec::new();
        for chunk in &chunks {
            assert_eq!(chunk[0], 0x16);
            let len = u16::from_be_bytes([chunk[3], chunk[4]]) as usize;
            assert_eq!(chunk.len(), 5 + len);
            reassembled.extend_from_slice(&chunk[5..]);
        }
        assert_eq!(reassembled, handshake);
    }
}
