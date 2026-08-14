//! Android edinim sonuçlarından JSON, rapor, timeline ve MFT çıktıları üretir.
use super::format::{Field, FieldType, MAGIC, MftBundleInfo, Record, RecordType, VERSION, now_ns};
use super::parsers::{
    build_logical_records, parse_getprop_line, parse_i64_prefix, read_text_file, trim_for_record,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Mantıksal Android kayıtlarından evidence.json, rapor, timeline ve korelasyon dosyalarını üretir.
pub fn write_logical_analysis_outputs(
    serial: &str,
    dir: &Path,
) -> Result<Vec<MftBundleInfo>, String> {
    let mut records = build_logical_records(dir);
    if records.is_empty() {
        records.push(Record::new(
            RecordType::Telemetry,
            vec![
                Field::string(0x01, "amele.mft.status"),
                Field::string(0x02, "no structured Android records were extracted"),
            ],
        ));
    }

    let mut outputs = Vec::new();
    outputs.push(write_json_file(
        dir,
        "evidence.json",
        &evidence_json(serial, &records),
        records.len(),
    )?);
    outputs.push(write_text_file(
        dir,
        "mobile_report.txt",
        &report_text(serial, &records),
        records.len(),
    )?);
    outputs.push(write_json_file(
        dir,
        "timeline.json",
        &timeline_json(&records),
        records.len(),
    )?);
    outputs.push(write_json_file(
        dir,
        "correlations.json",
        &correlations_json(&records),
        records.len(),
    )?);

    if let Some(profile) = device_profile_json(serial, dir) {
        outputs.push(write_json_file(dir, "device_profile.json", &profile, 1)?);
    }

    Ok(outputs)
}

pub(super) fn sha256_file(path: &Path) -> Result<String, String> {
    crate::hash::calculate_file_hash(path, crate::hash::HashAlgorithm::Sha256)
        .map_err(|e| e.to_string())
}

/// JSON değeri pretty formatla dosyaya yazar ve sidecar hash üretir.
fn write_json_file(
    dir: &Path,
    file_name: &str,
    value: &Value,
    record_count: usize,
) -> Result<MftBundleInfo, String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|err| format!("{file_name} olusturulamadi: {err}"))?;
    write_text_file(dir, file_name, &content, record_count)
}

/// Metin içeriğini dosyaya yazar ve sidecar hash üretir.
fn write_text_file(
    dir: &Path,
    file_name: &str,
    content: &str,
    record_count: usize,
) -> Result<MftBundleInfo, String> {
    let path = dir.join(file_name);
    fs::write(&path, content).map_err(|err| format!("{file_name} yazilamadi: {err}"))?;
    file_info_with_sidecar(dir, file_name, record_count)
}

/// Yazılan dosyanın boyut/hash/kayıt sayısı bilgisini MftBundleInfo olarak döndürür.
fn file_info_with_sidecar(
    dir: &Path,
    file_name: &str,
    record_count: usize,
) -> Result<MftBundleInfo, String> {
    let path = dir.join(file_name);
    let size = fs::metadata(&path)
        .map_err(|err| format!("{file_name} metadata okunamadi: {err}"))?
        .len();
    let sha256 = sha256_file(&path)?;
    fs::write(
        dir.join(format!("{file_name}.sha256")),
        format!("{sha256}  {file_name}\n"),
    )
    .map_err(|err| format!("{file_name} hash dosyasi yazilamadi: {err}"))?;
    Ok(MftBundleInfo {
        file_name: file_name.to_string(),
        size,
        sha256,
        record_count,
    })
}

/// Yapısal kayıt listesini evidence.json formatına çevirir.
fn evidence_json(serial: &str, records: &[Record]) -> Value {
    json!({
        "header": {
            "magic": MAGIC,
            "version": VERSION,
            "timestamp": now_ns(),
            "serial": serial,
        },
        "record_count": records.len(),
        "records": records.iter().map(record_json).collect::<Vec<_>>(),
    })
}

/// Tek MFT kaydını JSON field/raw alanlarına dönüştürür.
fn record_json(record: &Record) -> Value {
    let mut fields = Map::new();
    let mut raw = Map::new();

    for field in &record.fields {
        let name = field_name(record.record_type, field.id);
        raw.insert(name.clone(), Value::String(bytes_to_hex(&field.data)));
        fields.insert(name, field_value_json(field));
    }

    json!({
        "type": record_type_name(record.record_type),
        "timestamp": record.timestamp_ns,
        "fields": fields,
        "raw": raw,
    })
}

/// MFT field verisini tipine göre JSON değerine dönüştürür.
fn field_value_json(field: &Field) -> Value {
    match field.field_type {
        FieldType::String => Value::String(String::from_utf8_lossy(&field.data).into_owned()),
        FieldType::Int64 => {
            if field.data.len() >= 8 {
                let mut bytes = [0_u8; 8];
                bytes.copy_from_slice(&field.data[..8]);
                Value::Number(i64::from_le_bytes(bytes).into())
            } else {
                Value::Null
            }
        }
        FieldType::Bool => Value::Bool(field.data.first().copied().unwrap_or(0) != 0),
        FieldType::Binary => Value::String(bytes_to_hex(&field.data)),
    }
}

/// Yapısal kayıtları okunabilir mobil adli bilişim rapor metnine çevirir.
fn report_text(serial: &str, records: &[Record]) -> String {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for record in records {
        *counts
            .entry(record_type_name(record.record_type))
            .or_default() += 1;
    }

    let mut out = String::new();
    out.push_str("AMELE MOBILE FORENSIC REPORT\n");
    out.push_str("===========================\n\n");
    out.push_str(&format!("Device Serial: {serial}\n"));
    out.push_str(&format!(
        "Generated: {}\n",
        chrono::Local::now().to_rfc3339()
    ));
    out.push_str(&format!("Total Records: {}\n\n", records.len()));

    out.push_str("SUMMARY\n");
    out.push_str("-------\n");
    let mut names: Vec<&str> = counts.keys().copied().collect();
    names.sort_unstable();
    for name in names {
        out.push_str(&format!("{name:<14} {}\n", counts[name]));
    }

    write_report_section(
        &mut out,
        "CONTACTS",
        records,
        RecordType::Contact,
        &[0x02, 0x03, 0x04],
    );
    write_report_section(
        &mut out,
        "CALLS",
        records,
        RecordType::Call,
        &[0x01, 0x03, 0x04, 0x05],
    );
    write_report_section(
        &mut out,
        "SMS",
        records,
        RecordType::Sms,
        &[0x01, 0x02, 0x04],
    );
    write_report_section(
        &mut out,
        "MEDIA",
        records,
        RecordType::Media,
        &[0x01, 0x02, 0x04, 0x05],
    );
    write_report_section(
        &mut out,
        "PROCESSES",
        records,
        RecordType::ProcInfo,
        &[0x01, 0x02, 0x09],
    );
    write_report_section(
        &mut out,
        "NETWORK",
        records,
        RecordType::Network,
        &[0x01, 0x02, 0x03, 0x04],
    );
    write_report_section(
        &mut out,
        "MEMORY DUMPS",
        records,
        RecordType::MemoryDump,
        &[0x01, 0x04, 0x05],
    );
    write_report_section(
        &mut out,
        "ACCOUNTS",
        records,
        RecordType::Account,
        &[0x01, 0x02],
    );
    write_report_section(
        &mut out,
        "LOCATION",
        records,
        RecordType::Location,
        &[0x01, 0x02, 0x03, 0x05],
    );
    write_report_section(
        &mut out,
        "WIFI",
        records,
        RecordType::Wifi,
        &[0x01, 0x02, 0x04],
    );

    out
}

/// Rapor içinde belirli kayıt tipine ait kısa bölüm üretir.
fn write_report_section(
    out: &mut String,
    title: &str,
    records: &[Record],
    record_type: RecordType,
    field_ids: &[u8],
) {
    let matching: Vec<&Record> = records
        .iter()
        .filter(|record| record.record_type as u8 == record_type as u8)
        .take(100)
        .collect();
    if matching.is_empty() {
        return;
    }

    out.push_str("\n");
    out.push_str(title);
    out.push_str("\n");
    out.push_str(&"-".repeat(title.len()));
    out.push('\n');
    for record in matching {
        let parts: Vec<String> = field_ids
            .iter()
            .filter_map(|id| record_field_string(record, *id))
            .filter(|value| !value.is_empty())
            .map(|value| value.replace('\n', " "))
            .collect();
        if !parts.is_empty() {
            out.push_str("  ");
            out.push_str(&trim_for_record(&parts.join(" | "), 240));
            out.push('\n');
        }
    }
}

/// Kayıtlardan zaman sıralı timeline JSON üretir.
fn timeline_json(records: &[Record]) -> Value {
    let mut events = Vec::new();
    for record in records {
        if let Some(event) = timeline_event(record) {
            events.push(event);
        }
    }
    events.sort_by(|left, right| {
        right
            .get("timestamp")
            .and_then(Value::as_i64)
            .cmp(&left.get("timestamp").and_then(Value::as_i64))
    });
    json!({
        "event_count": events.len(),
        "events": events,
    })
}

/// Tek MFT kaydını timeline olayı olarak temsil edilebilir hale getirir.
fn timeline_event(record: &Record) -> Option<Value> {
    let event_type = record_type_name(record.record_type);
    let (timestamp, summary, severity) = match record.record_type {
        RecordType::Call => {
            let number = record_field_string(record, 0x01).unwrap_or_default();
            let duration = record_field_i64(record, 0x03).unwrap_or_default();
            let ts = record_field_i64(record, 0x04).unwrap_or(record.timestamp_ns);
            (
                normalize_event_timestamp(ts),
                format!("Call {number} ({duration}s)"),
                2,
            )
        }
        RecordType::Sms => {
            let address = record_field_string(record, 0x01).unwrap_or_default();
            let body = record_field_string(record, 0x02).unwrap_or_default();
            let ts = record_field_i64(record, 0x04).unwrap_or(record.timestamp_ns);
            (
                normalize_event_timestamp(ts),
                format!("SMS {address}: {}", trim_for_record(&body, 120)),
                2,
            )
        }
        RecordType::Contact => {
            let name = record_field_string(record, 0x02).unwrap_or_default();
            (record.timestamp_ns, format!("Contact {name}"), 1)
        }
        RecordType::Media => {
            let path = record_field_string(record, 0x01)
                .or_else(|| record_field_string(record, 0x05))
                .unwrap_or_default();
            let ts = record_field_i64(record, 0x03).unwrap_or(record.timestamp_ns);
            (normalize_event_timestamp(ts), format!("Media {path}"), 1)
        }
        RecordType::ProcInfo => {
            let name = record_field_string(record, 0x02).unwrap_or_default();
            (record.timestamp_ns, format!("Process {name}"), 2)
        }
        RecordType::Network => {
            let local = record_field_string(record, 0x01).unwrap_or_default();
            let remote = record_field_string(record, 0x02).unwrap_or_default();
            (
                record.timestamp_ns,
                format!("Network {local} -> {remote}"),
                3,
            )
        }
        RecordType::LogEntry => {
            let tag = record_field_string(record, 0x04).unwrap_or_default();
            let msg = record_field_string(record, 0x05).unwrap_or_default();
            (
                record.timestamp_ns,
                format!("{tag}: {}", trim_for_record(&msg, 140)),
                1,
            )
        }
        RecordType::MemoryDump => {
            let pid = record_field_string(record, 0x01).unwrap_or_default();
            let path = record_field_string(record, 0x05).unwrap_or_default();
            (
                record.timestamp_ns,
                format!("Memory dump PID {pid}: {path}"),
                4,
            )
        }
        RecordType::Notification => {
            let package = record_field_string(record, 0x01).unwrap_or_default();
            (record.timestamp_ns, format!("Notification {package}"), 2)
        }
        RecordType::Account => {
            let account = record_field_string(record, 0x01).unwrap_or_default();
            (record.timestamp_ns, format!("Account {account}"), 2)
        }
        RecordType::Location => {
            let app = record_field_string(record, 0x05).unwrap_or_default();
            (record.timestamp_ns, format!("Location {app}"), 4)
        }
        RecordType::Wifi => {
            let ssid = record_field_string(record, 0x01).unwrap_or_default();
            (record.timestamp_ns, format!("Wi-Fi {ssid}"), 2)
        }
        _ => return None,
    };

    Some(json!({
        "timestamp": timestamp,
        "type": event_type,
        "summary": summary,
        "severity": severity,
    }))
}

/// Kişi, arama ve SMS kayıtlarını telefon numarası bazında ilişkilendirir.
fn correlations_json(records: &[Record]) -> Value {
    #[derive(Default)]
    struct Correlation {
        contact_name: String,
        call_count: usize,
        sms_count: usize,
        last_contact: i64,
    }

    let mut rows: HashMap<String, Correlation> = HashMap::new();
    for record in records {
        match record.record_type {
            RecordType::Contact => {
                if let Some(phone) = record_field_string(record, 0x03) {
                    let row = rows.entry(normalize_phone(&phone)).or_default();
                    if row.contact_name.is_empty() {
                        row.contact_name = record_field_string(record, 0x02).unwrap_or_default();
                    }
                }
            }
            RecordType::Call => {
                if let Some(number) = record_field_string(record, 0x01) {
                    let row = rows.entry(normalize_phone(&number)).or_default();
                    row.call_count += 1;
                    row.last_contact = row.last_contact.max(normalize_event_timestamp(
                        record_field_i64(record, 0x04).unwrap_or(0),
                    ));
                }
            }
            RecordType::Sms => {
                if let Some(address) = record_field_string(record, 0x01) {
                    let row = rows.entry(normalize_phone(&address)).or_default();
                    row.sms_count += 1;
                    row.last_contact = row.last_contact.max(normalize_event_timestamp(
                        record_field_i64(record, 0x04).unwrap_or(0),
                    ));
                }
            }
            _ => {}
        }
    }

    let mut records: Vec<Value> = rows
        .into_iter()
        .filter(|(phone, row)| !phone.is_empty() && (row.call_count > 0 || row.sms_count > 0))
        .map(|(phone, row)| {
            json!({
                "phone_number": phone,
                "contact_name": row.contact_name,
                "call_count": row.call_count,
                "sms_count": row.sms_count,
                "last_contact": row.last_contact,
            })
        })
        .collect();
    records.sort_by(|left, right| {
        right
            .get("last_contact")
            .and_then(Value::as_i64)
            .cmp(&left.get("last_contact").and_then(Value::as_i64))
    });

    json!({
        "record_count": records.len(),
        "records": records,
    })
}

/// device_info.txt içinden cihaz profili JSON çıktısı üretir.
fn device_profile_json(serial: &str, dir: &Path) -> Option<Value> {
    let content = read_text_file(dir.join("device_info.txt"))?;
    let props: HashMap<String, String> = content.lines().filter_map(parse_getprop_line).collect();
    Some(json!({
        "serial": serial,
        "manufacturer": props.get("ro.product.manufacturer").cloned().unwrap_or_default(),
        "brand": props.get("ro.product.brand").cloned().unwrap_or_default(),
        "model": props.get("ro.product.model").cloned().unwrap_or_default(),
        "device": props.get("ro.product.device").cloned().unwrap_or_default(),
        "android_release": props.get("ro.build.version.release").cloned().unwrap_or_default(),
        "sdk": props.get("ro.build.version.sdk").cloned().unwrap_or_default(),
        "security_patch": props.get("ro.build.version.security_patch").cloned().unwrap_or_default(),
        "fingerprint": props.get("ro.build.fingerprint").cloned().unwrap_or_default(),
        "hardware": props.get("ro.hardware").cloned().unwrap_or_default(),
        "abi": props.get("ro.product.cpu.abi").cloned().unwrap_or_default(),
        "selinux": props.get("ro.build.selinux").cloned().unwrap_or_default(),
    }))
}

/// Kayıttaki alanı string olarak okur.
fn record_field_string(record: &Record, id: u8) -> Option<String> {
    record
        .fields
        .iter()
        .find(|field| field.id == id)
        .and_then(|field| match field.field_type {
            FieldType::String => Some(String::from_utf8_lossy(&field.data).into_owned()),
            FieldType::Int64 => record_field_i64_data(&field.data).map(|value| value.to_string()),
            FieldType::Bool => Some((field.data.first().copied().unwrap_or(0) != 0).to_string()),
            FieldType::Binary => Some(bytes_to_hex(&field.data)),
        })
}

/// Kayıttaki alanı i64 sayı olarak okur.
fn record_field_i64(record: &Record, id: u8) -> Option<i64> {
    record
        .fields
        .iter()
        .find(|field| field.id == id)
        .and_then(|field| match field.field_type {
            FieldType::Int64 => record_field_i64_data(&field.data),
            FieldType::String => parse_i64_prefix(&String::from_utf8_lossy(&field.data)),
            _ => None,
        })
}

/// Little-endian byte dizisini i64 değerine çevirir.
fn record_field_i64_data(data: &[u8]) -> Option<i64> {
    if data.len() < 8 {
        return None;
    }
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&data[..8]);
    Some(i64::from_le_bytes(bytes))
}

/// Saniye/milisaniye/nanosaniye karışık zamanları nanosaniyeye normalize eder.
fn normalize_event_timestamp(value: i64) -> i64 {
    if value <= 0 {
        return 0;
    }
    if value < 10_000_000_000 {
        value * 1_000_000_000
    } else if value < 10_000_000_000_000 {
        value * 1_000_000
    } else {
        value
    }
}

/// Telefon numarasını karşılaştırma için sadece rakam ve baştaki + karakterine indirger.
fn normalize_phone(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() || (ch == '+' && out.is_empty()) {
            out.push(ch);
        }
    }
    out
}

/// MFT kayıt tipini insan okunur ada çevirir.
fn record_type_name(record_type: RecordType) -> &'static str {
    match record_type {
        RecordType::Contact => "Contact",
        RecordType::Call => "Call",
        RecordType::Sms => "SMS",
        RecordType::Media => "Media",
        RecordType::ProcInfo => "ProcInfo",
        RecordType::Network => "Network",
        RecordType::LogEntry => "LogEntry",
        RecordType::MemoryDump => "MemoryDump",
        RecordType::MemInfo => "MemInfo",
        RecordType::Notification => "Notification",
        RecordType::Telemetry => "Telemetry",
        RecordType::UsageStat => "UsageStat",
        RecordType::Account => "Account",
        RecordType::Location => "Location",
        RecordType::Wifi => "WiFi",
    }
}

/// Kayıt tipi ve field id çiftini JSON alan adına çevirir.
fn field_name(record_type: RecordType, id: u8) -> String {
    let name = match record_type {
        RecordType::Contact => match id {
            0x01 => "contact_id",
            0x02 => "name",
            0x03 => "phone",
            0x04 => "email",
            0x05 => "type",
            0x06 => "raw_id",
            _ => "",
        },
        RecordType::Call => match id {
            0x01 => "number",
            0x02 => "type",
            0x03 => "duration",
            0x04 => "date",
            0x05 => "name",
            0x06 => "account_id",
            _ => "",
        },
        RecordType::Sms => match id {
            0x01 => "address",
            0x02 => "body",
            0x03 => "type",
            0x04 => "date",
            0x05 => "thread_id",
            0x06 => "read",
            0x07 => "service_center",
            _ => "",
        },
        RecordType::Media => match id {
            0x01 => "path",
            0x02 => "size",
            0x03 => "date_added",
            0x04 => "mime_type",
            0x05 => "title",
            0x07 => "width",
            0x08 => "height",
            0x09 => "duration",
            _ => "",
        },
        RecordType::ProcInfo => match id {
            0x01 => "pid",
            0x02 => "name",
            0x04 => "ppid",
            0x09 => "cmdline",
            _ => "",
        },
        RecordType::Network => match id {
            0x01 => "local_addr",
            0x02 => "remote_addr",
            0x03 => "state",
            0x04 => "protocol",
            _ => "",
        },
        RecordType::LogEntry => match id {
            0x01 => "pid",
            0x02 => "tid",
            0x03 => "priority",
            0x04 => "tag",
            0x05 => "message",
            _ => "",
        },
        RecordType::MemoryDump => match id {
            0x01 => "pid",
            0x02 => "start_addr",
            0x03 => "end_addr",
            0x04 => "perms",
            0x05 => "path",
            0x06 => "data",
            _ => "",
        },
        RecordType::MemInfo => match id {
            0x07 => "summary",
            _ => "",
        },
        RecordType::Notification => match id {
            0x01 => "package",
            0x05 => "tag",
            0x07 => "channel",
            _ => "",
        },
        RecordType::Telemetry => match id {
            0x01 => "key",
            0x02 => "value",
            _ => "",
        },
        RecordType::UsageStat => match id {
            0x01 => "package",
            0x02 => "last_time",
            0x03 => "total_time",
            0x04 => "launch_count",
            _ => "",
        },
        RecordType::Account => match id {
            0x01 => "name",
            0x02 => "type",
            _ => "",
        },
        RecordType::Location => match id {
            0x01 => "lat",
            0x02 => "lon",
            0x03 => "provider",
            0x04 => "time",
            0x05 => "app",
            _ => "",
        },
        RecordType::Wifi => match id {
            0x01 => "ssid",
            0x02 => "bssid",
            0x03 => "time",
            0x04 => "security",
            _ => "",
        },
    };
    if name.is_empty() {
        format!("field_0x{id:02x}")
    } else {
        name.to_string()
    }
}

/// Binary veriyi küçük harf hex stringe çevirir.
fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
