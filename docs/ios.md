# iOS Adli Bilişim Modülü / iOS Forensic Module

---

## Türkçe

### Genel Bakış

Amele iOS modülü, iTunes/Finder tarzı iOS backup klasörlerini `Manifest.db`
indeksine göre okunabilir bir dosya sistemi ağacına dönüştürür. Mantık
Backup2FS ile aynıdır: hashlenmiş düz backup dosyaları gerçek domain ve
relative path bilgilerine göre yeniden klasörlenir.

Bu entegrasyon Rust/native katmanda çalışır; Windows ve Linux üzerinde aynı
kod yolu kullanılır. Harici `.exe`, WPF arayüzü, Wine veya dotnet runtime
gerektirmez.

### Desteklenen Backup Tipi

| Tip | Durum | Açıklama |
|-----|-------|----------|
| Şifresiz iTunes/Finder backup | Aktif | `Manifest.db` doğrudan okunur ve dosyalar normalize edilir. |
| Önceden decrypt edilmiş backup | Aktif | Backup2FS veya eşdeğer araçla decrypt edilmiş klasör işlenebilir. |
| Şifreli iOS backup | Kontrollü uyarı | Native normalizer şimdilik şifre çözmez; önce şifresi kaldırılmış/decrypted klasör gerekir. |

### Çıktı Klasör Yapısı

```
~/Amele/Vakalar/{vaka}/ios/{backup_adi}_{tarih}/
├── ios_manifest.json
├── ios_manifest.json.sha256
├── extraction_log_{tarih}.csv
└── private/
    └── var/
        ├── mobile/
        ├── Keychains/
        ├── db/
        └── ...
```

### Üretilen Log

`extraction_log_*.csv` her `Manifest.db` girdisi için bir satır üretir:

```text
Timestamp,Status,Domain,RelativePath,FileID,OutputPath,SizeBytes,MD5,SHA1,SHA256
```

`Status` değerleri:

- `Copied`: Dosya kopyalandı ve seçili hashler hesaplandı.
- `Directory`: Manifest girdisi klasör olarak oluşturuldu.
- `Symlink`: Symlink girdisi kayda alındı.
- `Missing`: Backup içindeki hashlenmiş kaynak dosya bulunamadı.
- `Error`: Girdi işlenirken hata oluştu.

### Kullanım

1. Sol menüden **iOS Araçları** sayfasını açın.
2. `Manifest.db` içeren iTunes/Finder backup klasörünü seçin.
3. **Backup Profilini Oku** ile cihaz ve backup durumunu kontrol edin.
4. Vaka seçin veya yeni vaka oluşturun.
5. MD5/SHA1/SHA256 hash seçeneklerini belirleyin.
6. **Backup'ı Normalize Et** ile işlemi başlatın.

İşlem sırasında pause/resume/stop kontrolleri ve canlı log paneli kullanılabilir.

### CLI

```bash
amele ios-backup-profile /path/to/ios-backup
amele ios-backup-normalize /path/to/ios-backup Case_001
```

---

## English

### Overview

The Amele iOS module normalizes iTunes/Finder-style iOS backup folders into a
browsable file-system tree using the `Manifest.db` index. The semantics match
Backup2FS: flat hashed backup files are rebuilt by domain and relative path.

The integration runs in Amele's Rust/native layer, using the same code path on
Windows and Linux. It does not require an external `.exe`, WPF UI, Wine, or the
dotnet runtime.

### Supported Backup Types

| Type | Status | Description |
|------|--------|-------------|
| Unencrypted iTunes/Finder backup | Active | `Manifest.db` is read directly and files are normalized. |
| Already decrypted backup | Active | A folder decrypted by Backup2FS or an equivalent tool can be processed. |
| Encrypted iOS backup | Guarded warning | The native normalizer does not decrypt yet; provide an unencrypted/decrypted backup folder first. |

### Output Layout

```
~/Amele/Vakalar/{case}/ios/{backup_name}_{date}/
├── ios_manifest.json
├── ios_manifest.json.sha256
├── extraction_log_{date}.csv
└── private/
    └── var/
        ├── mobile/
        ├── Keychains/
        ├── db/
        └── ...
```

### CSV Log

`extraction_log_*.csv` writes one row per `Manifest.db` entry:

```text
Timestamp,Status,Domain,RelativePath,FileID,OutputPath,SizeBytes,MD5,SHA1,SHA256
```

`Status` values:

- `Copied`: File copied and selected hashes calculated.
- `Directory`: Directory entry created.
- `Symlink`: Symlink entry recorded.
- `Missing`: Hashed source file was missing from the backup.
- `Error`: Entry processing failed.

### Usage

1. Open **iOS Tools** from the sidebar.
2. Select the iTunes/Finder backup folder containing `Manifest.db`.
3. Use **Read Backup Profile** to inspect device and backup status.
4. Select or create a case.
5. Choose MD5/SHA1/SHA256 hash options.
6. Start **Normalize Backup**.

Pause/resume/stop controls and live logs are available during processing.

### CLI

```bash
amele ios-backup-profile /path/to/ios-backup
amele ios-backup-normalize /path/to/ios-backup Case_001
```
