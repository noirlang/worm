# Windows Adli Bilişim Modülü / Windows Forensic Module

---

## 🇹🇷 Türkçe

### Genel Bakış

Windows modülü iki farklı çalışma modunu destekler:

1. **Yerel İmaj Alma** — Amele'un çalıştığı Windows makinedeki disklerden doğrudan imaj alma
2. **Uzak İmaj Alma** — Hedef Windows makineye yerleştirilen `amele-win` ajanı üzerinden ağdan imaj ve RAM edinimi

### Yerel İmaj Alma

Administrator yetkisi gerektirir.

#### Disk İmajı

| Özellik | Açıklama |
|---------|----------|
| **Kaynak** | `\\.\PhysicalDrive0`, `\\.\PhysicalDrive1`, ... (0–31) |
| **Çıktı** | RAW (`.img`) bit-by-bit tam kopyası |
| **Hash** | SHA-256 (otomatik, `.sha256` sidecar dosyası) |
| **Boyut Algılama** | `DeviceIoControl(IOCTL_DISK_GET_LENGTH_INFO)` ile disk boyutu |
| **Dosya Açma** | `CreateFileW` + `GENERIC_READ` + `FILE_SHARE_READ/WRITE` |
| **Chunk Boyutu** | 4 MB (varsayılan) |
| **Kontrol** | Pause / Resume / Cancel desteği |
| **Kısmî Kurtarma** | Hata durumunda `.partial` dosya olarak saklanır |

**İş Akışı:**
1. `\\.\PhysicalDrive[0-31]` taranarak disk listesi oluşturulur
2. Kullanıcı disk ve vaka seçer
3. Bit-by-bit kopyalama başlatılır
4. SHA-256 hash eşzamanlı hesaplanır
5. `.img` + `.sha256` dosyaları vaka klasörüne yazılır

#### RAM İmajı (WinPMEM)

| Özellik | Açıklama |
|---------|----------|
| **Araç** | [go-winpmem](https://github.com/Velocidex/go-winpmem) (Velocidex) |
| **Dosya** | `go-winpmem_amd64_1.0-rc2_signed.exe` |
| **Yetki** | Administrator gerekli |
| **Çıktı** | AFF4 veya RAW bellek dökümü |
| **Arama Yolları** | `PATH`, çalışma dizini, `C:\Forensics\`, `C:\Tools\` |
| **Komut Varyantları** | 3 farklı CLI sözdizimi otomatik denenir |
| **İlerleme** | Çıktı dosyası boyutu izlenerek takip edilir |
| **Zaman Aşımı** | 1 saat |
| **Kontrol** | `Suspend-Process`/`Resume-Process` ile pause/resume |

**WinPMEM Komut Varyantları (sırasıyla denenir):**
```
1. winpmem.exe acquire <output_file>
2. winpmem.exe acquire --format raw <output_file>
```

### Uzak İmaj Alma (amele-win Ajanı)

Hedef Windows makineye `amele-win` ajanı yerleştirilir ve TCP üzerinden JSON protokolü ile iletişim kurulur.

#### Protokol

| Alan | Değer | Açıklama |
|------|-------|----------|
| Transport | TCP | JSON-over-TCP |
| Başlatma | `{"komut":"merhaba"}` | El sıkışma (handshake) |
| Kimlik Doğrulama | `guvenlik_anahtar_b64` | Base64 token (opsiyonel) |
| Veri Akışı | Binary stream | JSON kontrol mesajları arasında ham veri |

> **Not:** Protokol alanları Türkçe'dir (`komut`, `durum`, `diskler`, `boyut` vb.). Bu `amele-linux` ve `amele-win` ajanlarıyla geriye dönük uyumluluk için zorunludur.

#### Uzak Disk İmajı

| Komut | Açıklama |
|-------|----------|
| `disk_listele` | Hedef makinedeki diskleri listeler (`id`, `ad`, `boyut`) |
| `imaj_baslat` | Disk imaj transferini başlatır (RAW binary stream) |
| `edinim_kontrol` | Devam et / duraklat / iptal et |

**Toplanan Veri:**
- RAW disk imajı (bit-by-bit, `\\.\PhysicalDriveN`)
- SHA-256 + MD5 hash (ajan tarafından hesaplanır)
- İlerleme bilgisi (gerçek zamanlı)

#### Uzak RAM İmajı (WinPMEM)

| Komut | Açıklama |
|-------|----------|
| `winpmem_kontrol` | Ajanda WinPMEM mevcut mu? Administrator yetkisi var mı? RAM boyutu? |
| `ram_edinim_baslat` | WinPMEM'i uzaktan başlatır, RAM bellek dökümünü alır |
| `ram_dosya_indir` | Alınan RAM dökümünü ağ üzerinden indirir |

### Agentsız Uzak İmaj ve RAM Edinimi (OpenSSH / WinRM)

Hedef Windows makineye hiçbir ajan yüklemeden, yerel Windows OpenSSH servisi veya WinRM üzerinden doğrudan `PhysicalDrive` ve WinPMEM bellek edinimini gerçekleştirir.

#### Desteklenen İşlemler:
- **Disk Tarama:** `Get-CimInstance Win32_DiskDrive` veya `wmic diskdrive` ile tüm `PhysicalDrive` sürücülerini JSON olarak listeler.
- **Disk İmajı:** `PhysicalDrive[0-31]` üzerinden doğrudan ham binary stream ile disk kopyasını çeker.
- **RAM Edinimi:** `winpmem.exe` aracını uzaktan tetikleyerek bellek dökümünü canlı pipe ile yerel vakaya aktarır.
- **Format Desteği:** RAW (`.raw` / `.img`) veya AFF4 (`.aff4`) olarak vaka klasörüne kaydeder.
- **Canlı Hashleme:** İndirme esnasında eşzamanlı SHA-256 ve MD5 hash üretir.

#### CLI Komutları:
```bash
amele ssh-disks <ip> <kullanici> [port] [parola] [anahtar_yolu]
amele ssh-tool-check <ip> <kullanici> [port] [parola] [anahtar_yolu]
amele ssh-image <ip> <kullanici> <PhysicalDriveN> <cikti_klasoru> [vaka_adi] [port] [parola] [anahtar_yolu] [raw|aff4]
amele ssh-ram <ip> <kullanici> <cikti_klasoru> [vaka_adi] [port] [parola] [anahtar_yolu] [raw|aff4]
```

### Çıktı Klasör Yapısı

```
~/Amele/Vakalar/{vaka_adı}/
├── {ip}_{disk}_{tarih}.img          # Disk imajı
├── {ip}_{disk}_{tarih}.img.sha256   # SHA-256 hash
├── {ip}_{disk}_{tarih}.img.md5      # MD5 hash (uzak edinimde)
├── ram_{tarih}.raw                   # RAM dökümü
└── ram_{tarih}.raw.sha256            # RAM hash
```

### Desteklenen Disk Tipleri

| Tip | Yol | Açıklama |
|-----|-----|----------|
| Tüm fiziksel sürücüler | `\\.\PhysicalDrive[0-31]` | SATA, NVMe, USB, SAS — Windows'un gördüğü tüm fiziksel diskler |

---

## 🇬🇧 English

### Overview

The Windows module supports two operating modes:

1. **Local Imaging** — Direct disk imaging from the machine running Amele
2. **Remote Imaging** — Network-based disk and RAM acquisition via `amele-win` agent deployed on the target

### Local Imaging

Requires Administrator privileges.

#### Disk Image

| Feature | Description |
|---------|-------------|
| **Source** | `\\.\PhysicalDrive0`, `\\.\PhysicalDrive1`, ... (0–31) |
| **Output** | RAW (`.img`) bit-by-bit full copy |
| **Hash** | SHA-256 (automatic, `.sha256` sidecar file) |
| **Size Detection** | `DeviceIoControl(IOCTL_DISK_GET_LENGTH_INFO)` for disk size |
| **File Access** | `CreateFileW` + `GENERIC_READ` + `FILE_SHARE_READ/WRITE` |
| **Chunk Size** | 4 MB (default) |
| **Control** | Pause / Resume / Cancel support |
| **Partial Recovery** | Saved as `.partial` file on error |

**Workflow:**
1. `\\.\PhysicalDrive[0-31]` scanned to build disk list
2. User selects disk and case
3. Bit-by-bit copy begins
4. SHA-256 hash calculated simultaneously
5. `.img` + `.sha256` files written to case folder

#### RAM Image (WinPMEM)

| Feature | Description |
|---------|-------------|
| **Tool** | [go-winpmem](https://github.com/Velocidex/go-winpmem) (Velocidex) |
| **File** | `go-winpmem_amd64_1.0-rc2_signed.exe` |
| **Privilege** | Administrator required |
| **Output** | AFF4 or RAW memory dump |
| **Search Paths** | `PATH`, working directory, `C:\Forensics\`, `C:\Tools\` |
| **Command Variants** | 3 different CLI syntaxes tried automatically |
| **Progress** | Tracked by monitoring output file size |
| **Timeout** | 1 hour |
| **Control** | Pause/resume via `Suspend-Process`/`Resume-Process` |

**WinPMEM command variants (tried in order):**
```
1. winpmem.exe acquire <output_file>
2. winpmem.exe acquire --format raw <output_file>
```

### Remote Imaging (amele-win Agent)

A `amele-win` agent is deployed on the target Windows machine, communicating over TCP with a JSON protocol.

#### Protocol

| Field | Value | Description |
|-------|-------|-------------|
| Transport | TCP | JSON-over-TCP |
| Handshake | `{"komut":"merhaba"}` | Initial hello |
| Authentication | `guvenlik_anahtar_b64` | Base64 token (optional) |
| Data Transfer | Binary stream | Raw data between JSON control messages |

> **Note:** Protocol field names are in Turkish (`komut`, `durum`, `diskler`, `boyut`, etc.). This is mandatory for backward compatibility with `amele-linux` and `amele-win` agents.

#### Remote Disk Image

| Command | Description |
|---------|-------------|
| `disk_listele` | Lists disks on the target (`id`, `ad`, `boyut`) |
| `imaj_baslat` | Starts disk image transfer (RAW binary stream) |
| `edinim_kontrol` | Continue / pause / cancel |

**Collected Data:**
- RAW disk image (bit-by-bit, `\\.\PhysicalDriveN`)
- SHA-256 + MD5 hash (computed by the agent)
- Real-time progress reporting

#### Remote RAM Image (WinPMEM)

| Command | Description |
|---------|-------------|
| `winpmem_kontrol` | Is WinPMEM present on agent? Administrator privilege? RAM size? |
| `ram_edinim_baslat` | Starts WinPMEM remotely, captures RAM dump |
| `ram_dosya_indir` | Downloads captured RAM dump over the network |

### Agentless Remote Acquisition (OpenSSH / WinRM)

Directly acquire remote disks and memory without installing any agent on the target Windows system using native OpenSSH or WinRM.

#### Supported Operations:
- **Disk Listing:** Enumerates physical disks via `Get-CimInstance Win32_DiskDrive` or `wmic` formatted as JSON.
- **Disk Acquisition:** Pipes raw binary streams from `\\.\PhysicalDrive[0-31]` directly over SSH into local working files.
- **RAM Acquisition:** Streams memory dumps remotely via `winpmem.exe` pipe.
- **Output Formats:** Bit-by-bit RAW (`.raw` / `.img`) or forensic AFF4 (`.aff4`).
- **Live Hashing:** Generates SHA-256 and MD5 hashes simultaneously on the fly.

#### CLI Commands:
```bash
amele ssh-disks <ip> <user> [port] [password] [key_path]
amele ssh-tool-check <ip> <user> [port] [password] [key_path]
amele ssh-image <ip> <user> <PhysicalDriveN> <out_dir> [case_name] [port] [password] [key_path] [raw|aff4]
amele ssh-ram <ip> <user> <out_dir> [case_name] [port] [password] [key_path] [raw|aff4]
```

### Output Folder Structure

```
~/Amele/Vakalar/{case_name}/
├── {ip}_{disk}_{date}.img          # Disk image
├── {ip}_{disk}_{date}.img.sha256   # SHA-256 hash
├── {ip}_{disk}_{date}.img.md5      # MD5 hash (remote acquisition)
├── ram_{date}.raw                   # RAM dump
└── ram_{date}.raw.sha256            # RAM hash
```

### Supported Disk Types

| Type | Path | Description |
|------|------|-------------|
| All physical drives | `\\.\PhysicalDrive[0-31]` | SATA, NVMe, USB, SAS — all physical disks visible to Windows |
