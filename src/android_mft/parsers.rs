//! Android metin/JSON çıktılarından yapılandırılmış MFT kayıtları çıkarır.
use super::format::{Field, MAX_RECORDS_PER_SOURCE, MAX_TEXT_INPUT, Record, RecordType};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Mantıksal Android edinim klasöründeki bilinen çıktılardan MFT kayıtları üretir.
pub(super) fn build_logical_records(dir: &Path) -> Vec<Record> {
    let mut records = Vec::new();

    if let Some(content) = read_text_file(dir.join("device_info.txt")) {
        records.extend(parse_getprop_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("dumpsys_usagestats.txt")) {
        records.extend(parse_usage_stat_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("dumpsys_account.txt")) {
        records.extend(parse_account_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("dumpsys_wifi.txt")) {
        records.extend(parse_wifi_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("dumpsys_location.txt")) {
        records.extend(parse_location_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("dumpsys_meminfo.txt")) {
        records.push(Record::new(
            RecordType::MemInfo,
            vec![Field::string(0x07, trim_for_record(&content, 32 * 1024))],
        ));
    }
    if let Some(content) = read_text_file(dir.join("root_status.txt")) {
        records.extend(parse_status_telemetry_records("root_status", &content));
    }
    if let Some(content) = read_text_file(dir.join("procfs_summary.txt")) {
        records.push(Record::new(
            RecordType::MemInfo,
            vec![Field::string(0x07, trim_for_record(&content, 32 * 1024))],
        ));
    }
    if let Some(content) = read_text_file(dir.join("debug_heap_dumps").join("heapdump_log.tsv")) {
        records.extend(parse_heapdump_log_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("dumpsys_notification.txt")) {
        records.extend(parse_notification_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("processes.txt")) {
        records.extend(parse_process_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("network_info.txt")) {
        records.extend(parse_network_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("logcat.txt")) {
        records.extend(parse_logcat_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("content_sms.txt")) {
        records.extend(parse_sms_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("content_calls.txt")) {
        records.extend(parse_call_records(&content));
    }
    if let Some(content) = read_text_file(dir.join("content_contacts.txt")) {
        records.extend(parse_contact_records(&content));
    }
    for file in [
        "content_media_images.txt",
        "content_media_videos.txt",
        "content_media_audio.txt",
        "content_media_files.txt",
    ] {
        if let Some(content) = read_text_file(dir.join(file)) {
            records.extend(parse_media_records(&content));
        }
    }

    records
}

/// Büyük metin çıktısını üst sınırla okuyarak bellek taşmasını önler.
pub(super) fn read_text_file(path: impl AsRef<Path>) -> Option<String> {
    let path = path.as_ref();
    let mut file = File::open(path).ok()?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_TEXT_INPUT)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// getprop çıktısından cihaz/build/şifreleme telemetri kayıtlarını çıkarır.
fn parse_getprop_records(content: &str) -> Vec<Record> {
    let prefixes = [
        "ro.product",
        "ro.build",
        "ro.hardware",
        "ro.serialno",
        "gsm.version",
        "ro.crypto",
        "ro.secure",
    ];
    content
        .lines()
        .filter_map(parse_getprop_line)
        .filter(|(key, value)| {
            !value.is_empty() && prefixes.iter().any(|prefix| key.starts_with(prefix))
        })
        .map(|(key, value)| {
            Record::new(
                RecordType::Telemetry,
                vec![Field::string(0x01, key), Field::string(0x02, value)],
            )
        })
        .collect()
}

/// Tek getprop satırını key/value çiftine çevirir.
pub(super) fn parse_getprop_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') || !trimmed.contains("]: [") {
        return None;
    }
    let (key, value) = trimmed.split_once("]: [")?;
    Some((
        key.trim_start_matches('[').to_string(),
        value.trim_end_matches(']').to_string(),
    ))
}

/// dumpsys usagestats içinden uygulama kullanım kayıtlarını çıkarır.
fn parse_usage_stat_records(content: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut seen = HashSet::new();
    for line in content.lines().map(str::trim) {
        if records.len() >= MAX_RECORDS_PER_SOURCE {
            break;
        }
        if !(line.contains("package=") || line.contains("packageName=")) {
            continue;
        }
        let package = extract_value(line, "package")
            .or_else(|| extract_value(line, "packageName"))
            .unwrap_or_default();
        if package.is_empty() || !seen.insert(package.clone()) {
            continue;
        }
        let mut fields = vec![Field::string(0x01, package)];
        if let Some(last) =
            extract_value(line, "lastTime").or_else(|| extract_value(line, "lastTimeUsed"))
        {
            if let Some(value) = parse_i64_prefix(&last) {
                fields.push(Field::int64(0x02, value));
            }
        }
        if let Some(total) = extract_value(line, "totalTime")
            .or_else(|| extract_value(line, "totalTimeInForeground"))
        {
            if let Some(value) = parse_duration_to_ms(&total) {
                fields.push(Field::int64(0x03, value));
            }
        }
        records.push(Record::new(RecordType::UsageStat, fields));
    }
    records
}

/// dumpsys account çıktısından hesap adı ve tip kayıtlarını çıkarır.
fn parse_account_records(content: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut in_accounts = false;
    let mut seen = HashSet::new();

    for line in content.lines().map(str::trim) {
        if records.len() >= MAX_RECORDS_PER_SOURCE {
            break;
        }
        if line.starts_with("Accounts:") {
            in_accounts = true;
            continue;
        }
        if !in_accounts {
            continue;
        }
        if !(line.starts_with("Account {name=") || line.starts_with("Account {")) {
            if line.is_empty() || !line.starts_with("Account") {
                in_accounts = false;
            }
            continue;
        }
        let fields = parse_braced_fields(line);
        let name = fields.get("name").cloned().unwrap_or_default();
        let account_type = fields.get("type").cloned().unwrap_or_default();
        let key = format!("{name}\0{account_type}");
        if (name.is_empty() && account_type.is_empty()) || !seen.insert(key) {
            continue;
        }
        records.push(Record::new(
            RecordType::Account,
            vec![Field::string(0x01, name), Field::string(0x02, account_type)],
        ));
    }

    records
}

/// dumpsys wifi çıktısından yapılandırılmış ve bağlantı Wi-Fi kayıtlarını çıkarır.
fn parse_wifi_records(content: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut seen = HashSet::new();
    let mut in_configured = false;

    for raw in content.lines() {
        if records.len() >= MAX_RECORDS_PER_SOURCE {
            break;
        }
        let line = raw.trim();
        if line.starts_with("Configured networks:") || line.contains("NetworkList:") {
            in_configured = true;
            continue;
        }
        if line.is_empty() {
            in_configured = false;
            continue;
        }

        let candidate = if in_configured && line.contains("SSID:") {
            parse_configured_wifi_line(line)
        } else if line.contains("SSID:") && line.contains("BSSID:") {
            parse_connection_wifi_line(line)
        } else {
            None
        };

        let Some((ssid, bssid, security)) = candidate else {
            continue;
        };
        let key = format!("{ssid}\0{bssid}");
        if (ssid.is_empty() && bssid.is_empty()) || !seen.insert(key) {
            continue;
        }
        let mut fields = Vec::new();
        push_str_field(&mut fields, 0x01, &ssid);
        push_str_field(&mut fields, 0x02, &bssid);
        push_str_field(&mut fields, 0x04, &security);
        records.push(Record::new(RecordType::Wifi, fields));
    }

    records
}

/// dumpsys location çıktısından son konum ve uygulama konum isteklerini çıkarır.
fn parse_location_records(content: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut seen = HashSet::new();
    let mut in_last_known = false;
    let mut in_requests = false;

    for raw in content.lines() {
        if records.len() >= MAX_RECORDS_PER_SOURCE {
            break;
        }
        let line = raw.trim();
        if line.starts_with("Last Known Locations:") {
            in_last_known = true;
            in_requests = false;
            continue;
        }
        if line.contains("Location Requests:") || line.contains("Active Records:") {
            in_last_known = false;
            in_requests = true;
            continue;
        }
        if line.is_empty() {
            in_last_known = false;
            in_requests = false;
            continue;
        }

        let mut fields = Vec::new();
        if in_last_known {
            if let Some((provider, lat, lon)) = parse_last_known_location(line) {
                fields.push(Field::string(0x01, lat));
                fields.push(Field::string(0x02, lon));
                push_str_field(&mut fields, 0x03, &provider);
            }
        } else if in_requests {
            if let Some(app) = extract_package_name(line) {
                push_str_field(&mut fields, 0x05, &app);
                if line.contains("network") {
                    fields.push(Field::string(0x03, "network"));
                } else if line.contains("gps") {
                    fields.push(Field::string(0x03, "gps"));
                }
            }
        }

        if !fields.is_empty() {
            let key = fields
                .iter()
                .map(|field| String::from_utf8_lossy(&field.data).into_owned())
                .collect::<Vec<_>>()
                .join("\0");
            if seen.insert(key) {
                records.push(Record::new(RecordType::Location, fields));
            }
        }
    }

    records
}

/// Bildirim dumpsys çıktısından paket, kanal ve tag kayıtlarını çıkarır.
fn parse_notification_records(content: &str) -> Vec<Record> {
    content
        .lines()
        .filter(|line| line.contains("NotificationRecord") || line.contains("pkg="))
        .take(MAX_RECORDS_PER_SOURCE)
        .filter_map(|line| {
            let package = extract_value(line, "pkg").or_else(|| extract_package_name(line));
            let package = package?;
            let mut fields = vec![Field::string(0x01, package)];
            if let Some(channel) = extract_value(line, "channel") {
                fields.push(Field::string(0x07, channel));
            }
            if let Some(tag) = extract_value(line, "tag") {
                fields.push(Field::string(0x05, tag));
            }
            Some(Record::new(RecordType::Notification, fields))
        })
        .collect()
}

/// ps çıktısını proses MFT kayıtlarına dönüştürür.
fn parse_process_records(content: &str) -> Vec<Record> {
    let mut records = Vec::new();
    for line in content.lines().skip(1) {
        if records.len() >= MAX_RECORDS_PER_SOURCE {
            break;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let pid_index = parts.iter().position(|part| part.parse::<i64>().is_ok());
        let Some(pid_index) = pid_index else {
            continue;
        };
        let mut fields = Vec::new();
        if let Ok(pid) = parts[pid_index].parse::<i64>() {
            fields.push(Field::int64(0x01, pid));
        }
        if let Some(ppid) = parts
            .get(pid_index + 1)
            .and_then(|value| value.parse::<i64>().ok())
        {
            fields.push(Field::int64(0x04, ppid));
        }
        if let Some(name) = parts.last() {
            fields.push(Field::string(0x02, *name));
            fields.push(Field::string(0x09, *name));
        }
        records.push(Record::new(RecordType::ProcInfo, fields));
    }
    records
}

/// Ağ özet satırlarını protokol, local, remote ve state alanlarına ayırır.
fn parse_network_records(content: &str) -> Vec<Record> {
    content
        .lines()
        .filter(|line| line.contains("tcp") || line.contains("udp"))
        .take(MAX_RECORDS_PER_SOURCE)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                return None;
            }
            let mut fields = vec![Field::string(0x04, parts[0])];
            push_str_field(&mut fields, 0x01, parts.get(3).copied().unwrap_or_default());
            push_str_field(&mut fields, 0x02, parts.get(4).copied().unwrap_or_default());
            push_str_field(&mut fields, 0x03, parts.get(5).copied().unwrap_or_default());
            Some(Record::new(RecordType::Network, fields))
        })
        .collect()
}

/// logcat satırlarını PID, TID, öncelik, tag ve mesaj alanlarına böler.
fn parse_logcat_records(content: &str) -> Vec<Record> {
    content
        .lines()
        .take(MAX_RECORDS_PER_SOURCE)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                return None;
            }
            let pid = parts.get(2).and_then(|value| value.parse::<i64>().ok())?;
            let tid = parts
                .get(3)
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0);
            let priority = parts.get(4).copied().unwrap_or_default();
            let rest = parts[5..].join(" ");
            let (tag, message) = rest
                .split_once(':')
                .map(|(tag, message)| (tag.trim(), message.trim()))
                .unwrap_or(("", rest.trim()));
            Some(Record::new(
                RecordType::LogEntry,
                vec![
                    Field::int64(0x01, pid),
                    Field::int64(0x02, tid),
                    Field::string(0x03, priority),
                    Field::string(0x04, tag),
                    Field::string(0x05, message),
                ],
            ))
        })
        .collect()
}

/// content://sms sorgu çıktısını SMS kayıtlarına dönüştürür.
fn parse_sms_records(content: &str) -> Vec<Record> {
    parse_content_rows(content)
        .into_iter()
        .take(MAX_RECORDS_PER_SOURCE)
        .filter_map(|row| {
            let mut fields = Vec::new();
            push_row_str(&mut fields, 0x01, &row, &["address", "number"]);
            push_row_str(&mut fields, 0x02, &row, &["body", "text"]);
            push_row_int(&mut fields, 0x03, &row, &["type"]);
            push_row_int(&mut fields, 0x04, &row, &["date"]);
            push_row_int(&mut fields, 0x05, &row, &["thread_id"]);
            if let Some(read) = row.get("read").and_then(|value| parse_i64_prefix(value)) {
                fields.push(Field::bool(0x06, read != 0));
            }
            push_row_str(&mut fields, 0x07, &row, &["service_center"]);
            (!fields.is_empty()).then(|| Record::new(RecordType::Sms, fields))
        })
        .collect()
}

/// content://call_log sorgu çıktısını arama kayıtlarına dönüştürür.
fn parse_call_records(content: &str) -> Vec<Record> {
    parse_content_rows(content)
        .into_iter()
        .take(MAX_RECORDS_PER_SOURCE)
        .filter_map(|row| {
            let mut fields = Vec::new();
            push_row_str(&mut fields, 0x01, &row, &["number", "phone_number"]);
            push_row_int(&mut fields, 0x02, &row, &["type"]);
            push_row_int(&mut fields, 0x03, &row, &["duration"]);
            push_row_int(&mut fields, 0x04, &row, &["date"]);
            push_row_str(&mut fields, 0x05, &row, &["name", "cached_name"]);
            push_row_str(
                &mut fields,
                0x06,
                &row,
                &["subscription_component_name", "account_id"],
            );
            (!fields.is_empty()).then(|| Record::new(RecordType::Call, fields))
        })
        .collect()
}

/// contacts provider çıktısını kişi kayıtlarına dönüştürür.
fn parse_contact_records(content: &str) -> Vec<Record> {
    parse_content_rows(content)
        .into_iter()
        .take(MAX_RECORDS_PER_SOURCE)
        .filter_map(|row| {
            let mut fields = Vec::new();
            push_row_str(&mut fields, 0x01, &row, &["_id", "contact_id"]);
            push_row_str(&mut fields, 0x02, &row, &["display_name", "name"]);
            push_row_str(&mut fields, 0x03, &row, &["data1", "number", "phone"]);
            push_row_str(&mut fields, 0x04, &row, &["email", "data4"]);
            push_row_str(&mut fields, 0x05, &row, &["mimetype", "type"]);
            push_row_str(&mut fields, 0x06, &row, &["raw_contact_id"]);
            (!fields.is_empty()).then(|| Record::new(RecordType::Contact, fields))
        })
        .collect()
}

/// media provider çıktısını medya dosyası kayıtlarına dönüştürür.
fn parse_media_records(content: &str) -> Vec<Record> {
    parse_content_rows(content)
        .into_iter()
        .take(MAX_RECORDS_PER_SOURCE)
        .filter_map(|row| {
            let mut fields = Vec::new();
            push_row_str(
                &mut fields,
                0x01,
                &row,
                &["_data", "relative_path", "document_id"],
            );
            push_row_int(&mut fields, 0x02, &row, &["_size", "size"]);
            push_row_int(&mut fields, 0x03, &row, &["date_added", "date_modified"]);
            push_row_str(&mut fields, 0x04, &row, &["mime_type"]);
            push_row_str(&mut fields, 0x05, &row, &["_display_name", "title"]);
            push_row_int(&mut fields, 0x07, &row, &["width"]);
            push_row_int(&mut fields, 0x08, &row, &["height"]);
            push_row_int(&mut fields, 0x09, &row, &["duration"]);
            (!fields.is_empty()).then(|| Record::new(RecordType::Media, fields))
        })
        .collect()
}

/// root/procfs gibi başlıklı metinleri telemetri kayıtlarına böler.
fn parse_status_telemetry_records(prefix: &str, content: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut current_label = String::new();
    let mut current_value = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("===") && trimmed.ends_with("===") {
            if !current_label.is_empty() && !current_value.trim().is_empty() {
                records.push(Record::new(
                    RecordType::Telemetry,
                    vec![
                        Field::string(0x01, format!("{prefix}.{}", current_label)),
                        Field::string(0x02, trim_for_record(current_value.trim(), 4096)),
                    ],
                ));
            }
            current_label = trimmed.trim_matches('=').trim().replace(' ', "_");
            current_value.clear();
        } else if !trimmed.is_empty() {
            current_value.push_str(line);
            current_value.push('\n');
        }
    }

    if !current_label.is_empty() && !current_value.trim().is_empty() {
        records.push(Record::new(
            RecordType::Telemetry,
            vec![
                Field::string(0x01, format!("{prefix}.{}", current_label)),
                Field::string(0x02, trim_for_record(current_value.trim(), 4096)),
            ],
        ));
    }

    records
}

/// HPROF heap dump logundan başarılı dump kayıtlarını çıkarır.
fn parse_heapdump_log_records(content: &str) -> Vec<Record> {
    content
        .lines()
        .skip(1)
        .take(MAX_RECORDS_PER_SOURCE)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 4 || parts[2].ends_with("_failed") || parts[2] == "pid_not_running" {
                return None;
            }
            let pid = parts[1].parse::<i64>().ok()?;
            Some(Record::new(
                RecordType::MemoryDump,
                vec![
                    Field::int64(0x01, pid),
                    Field::string(0x04, "hprof"),
                    Field::string(0x05, format!("debug_heap_dumps/{}", parts[2])),
                ],
            ))
        })
        .collect()
}

/// Android content query Row satırlarını key/value map listesine dönüştürür.
fn parse_content_rows(content: &str) -> Vec<HashMap<String, String>> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let row_start = line.find("Row:")?;
            let payload = &line[row_start + 4..];
            let payload = payload.trim_start();
            let payload = payload
                .split_once(' ')
                .map(|(_, rest)| rest)
                .unwrap_or(payload);
            let separator = if payload.contains(", ") { ", " } else { " " };
            let mut fields = HashMap::new();
            for part in payload.split(separator) {
                let Some((key, value)) = part.split_once('=') else {
                    continue;
                };
                let value = value.trim().trim_matches('"');
                if !value.is_empty() && value != "null" {
                    fields.insert(key.trim().to_string(), value.to_string());
                }
            }
            (!fields.is_empty()).then_some(fields)
        })
        .collect()
}

/// Süslü parantezli dumpsys alanlarını key/value map olarak ayrıştırır.
fn parse_braced_fields(line: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let Some(start) = line.find('{') else {
        return fields;
    };
    let Some(end) = line[start + 1..].find('}') else {
        return fields;
    };
    let content = &line[start + 1..start + 1 + end];
    for part in content.split(',') {
        let Some((key, value)) = part.trim().split_once('=') else {
            continue;
        };
        let value = value.trim();
        if !value.is_empty() && value != "null" {
            fields.insert(key.trim().to_string(), value.to_string());
        }
    }
    fields
}

/// Configured networks satırından SSID/BSSID/güvenlik bilgisini çıkarır.
fn parse_configured_wifi_line(line: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let mut ssid = String::new();
    let mut bssid = String::new();
    let mut security = String::new();
    for (index, part) in parts.iter().enumerate() {
        if *part == "SSID:" {
            ssid = parts
                .get(index + 1)
                .map(|value| value.trim_matches('"').to_string())
                .unwrap_or_default();
        } else if *part == "BSSID:" {
            bssid = parts
                .get(index + 1)
                .filter(|value| **value != "null" && **value != "any")
                .map(|value| (*value).to_string())
                .unwrap_or_default();
        } else if part.contains("WPA") || part.contains("WEP") || part.contains("SAE") {
            security = (*part).to_string();
        }
    }
    normalize_wifi_tuple(ssid, bssid, security)
}

/// Aktif Wi-Fi bağlantı satırından SSID/BSSID bilgisini çıkarır.
fn parse_connection_wifi_line(line: &str) -> Option<(String, String, String)> {
    let mut ssid = String::new();
    let mut bssid = String::new();
    for part in line.split(',') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("SSID:") {
            ssid = value.trim().trim_matches('"').to_string();
        } else if let Some(value) = part.strip_prefix("BSSID:") {
            let value = value.trim();
            if value != "null" && value != "any" {
                bssid = value.to_string();
            }
        }
    }
    normalize_wifi_tuple(ssid, bssid, String::new())
}

/// Boş ve bilinmeyen Wi-Fi değerlerini temizleyip geçerli tuple döndürür.
fn normalize_wifi_tuple(
    mut ssid: String,
    bssid: String,
    security: String,
) -> Option<(String, String, String)> {
    if ssid == "<unknown ssid>" {
        ssid.clear();
    }
    if ssid.is_empty() && bssid.is_empty() {
        None
    } else {
        Some((ssid, bssid, security))
    }
}

/// Last Known Locations satırından provider, latitude ve longitude değerlerini çıkarır.
fn parse_last_known_location(line: &str) -> Option<(String, String, String)> {
    let (provider, rest) = line.split_once(": Location[")?;
    let mut parts = rest.split_whitespace();
    let _provider_name = parts.next()?;
    let coords = parts.next()?;
    let (lat, lon) = coords.split_once(',')?;
    if lat.parse::<f64>().is_err() || lon.parse::<f64>().is_err() {
        return None;
    }
    Some((
        provider.trim().to_string(),
        lat.to_string(),
        lon.to_string(),
    ))
}

/// Serbest metin içinde Android paket adına benzeyen ilk kelimeyi bulur.
fn extract_package_name(line: &str) -> Option<String> {
    line.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .find(|word| {
            (word.starts_with("com.") || word.starts_with("org.") || word.starts_with("net."))
                && word.contains('.')
        })
        .map(ToOwned::to_owned)
}

/// key=value biçimindeki alanı satırdan okur.
fn extract_value(line: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    if let Some(rest) = rest.strip_prefix('"') {
        return rest.split_once('"').map(|(value, _)| value.to_string());
    }
    let value = rest
        .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ']')
        .next()
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    (!value.is_empty() && value != "null").then_some(value)
}

/// Metnin başındaki sayısal bölümü i64 olarak parse eder.
pub(super) fn parse_i64_prefix(value: &str) -> Option<i64> {
    let digits: String = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect();
    digits.parse().ok()
}

/// Süre değerini milisaniye cinsinden i64 sayıya indirger.
fn parse_duration_to_ms(value: &str) -> Option<i64> {
    if let Some(number) = parse_i64_prefix(value) {
        return Some(number);
    }
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() >= 2 && parts.iter().all(|part| part.parse::<i64>().is_ok()) {
        let mut total = 0_i64;
        for part in parts {
            total = total * 60 + part.parse::<i64>().ok()?;
        }
        return Some(total * 1000);
    }
    None
}

/// Satır mapinden ilk bulunan string alanı MFT field listesine ekler.
fn push_row_str(fields: &mut Vec<Field>, id: u8, row: &HashMap<String, String>, keys: &[&str]) {
    if let Some(value) = keys.iter().find_map(|key| row.get(*key)) {
        push_str_field(fields, id, value);
    }
}

/// Satır mapinden ilk bulunan sayı alanı MFT field listesine ekler.
fn push_row_int(fields: &mut Vec<Field>, id: u8, row: &HashMap<String, String>, keys: &[&str]) {
    if let Some(value) = keys
        .iter()
        .find_map(|key| row.get(*key).and_then(|value| parse_i64_prefix(value)))
    {
        fields.push(Field::int64(id, value));
    }
}

/// Boş olmayan string değerini MFT field listesine ekler.
fn push_str_field(fields: &mut Vec<Field>, id: u8, value: &str) {
    if !value.trim().is_empty() {
        fields.push(Field::string(id, value.trim()));
    }
}

/// Çok uzun metni kayıt limitine indirir ve kırpıldığını belirtir.
pub(super) fn trim_for_record(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let safe_end = value
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|idx| *idx <= max_len)
        .last()
        .unwrap_or(0);
    let mut out = value[..safe_end].to_string();
    out.push_str("\n[truncated]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_parser_reads_dumpsys_account_records() {
        let input = "\
Accounts: 2
  Account {name=user@example.com, type=com.google}
  Account {name=15551234567, type=com.whatsapp}
";
        let records = parse_account_records(input);
        assert_eq!(records.len(), 2);
        assert!(
            records[0]
                .fields
                .iter()
                .any(|f| f.data == b"user@example.com")
        );
    }

    #[test]
    fn content_row_parser_keeps_android_query_fields() {
        let rows = parse_content_rows(
            "Row: 0 _id=1, address=5551234, body=hello, date=1710000000000, read=1",
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("address").unwrap(), "5551234");
        assert_eq!(rows[0].get("body").unwrap(), "hello");
    }

    #[test]
    fn trim_for_record_preserves_utf8_boundaries() {
        let trimmed = trim_for_record("Merhaba arkadaşlar", 12);
        assert!(trimmed.ends_with("[truncated]"));
    }
}
