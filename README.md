<div align="center">

<img src="ui/assets/logo/logo.png" alt="Amele Logo" width="120" />

# Amele Forensic Tool

*Windows, Linux, Android, and iOS unified digital forensics platform. Disk, RAM, logical acquisition, and backup analysis.*

[Website](https://amele.noirlang.tr) | [Releases](https://github.com/noirlang/amele/releases) | [Contributing](CONTRIBUTING.md) | [Security](SECURITY.md) | [Linux Agent](https://github.com/noirlang/amele-linux) | [Windows Agent](https://github.com/noirlang/amele-win)

<img src="amele.gif" alt="Amele Forensic Tool Demo" width="700" />

</div>

## Overview

Amele is a desktop forensic acquisition tool for authorized investigations. It brings disk imaging, memory acquisition, Android collection, iOS backup normalization, hash verification, case output handling, image viewing, and reporting into one native application.

The app runs as a real desktop window on Linux and Windows.

## Features

- **Local disk acquisition:** create raw disk images from local disks or image files.
- **Remote disk acquisition:** collect disk images through the Linux and Windows agents.
- **Local memory acquisition:** capture RAM with AVML on Linux and WinPMEM on Windows.
- **Remote memory acquisition:** start, pause, resume, stop, track, and download RAM dumps from agents.
- **Android tools:** check ADB, list devices, collect logical data, collect filesystem data, capture volatile data, and analyze Android case outputs.
- **iOS tools:** normalize unencrypted or already decrypted iTunes/Finder backups into a browsable Backup2FS-style file system, with per-file MD5/SHA1/SHA256 CSV logging.
- **Docker tools:** audit container escape risks, scan environment variables for exposed secrets/API keys, extract raw configs and logs, package runtime Overlay2 UpperDir drift layers, and stream remote container evidence via Linux Agent.
- **Online profiles:** connect an `amele.noirlang.tr` account, display synced roles such as BDFL or platform maintainer, and require verified membership for Android/iOS tools.
- **Case management:** store acquisitions, notes, hashes, Android outputs, iOS outputs, reports, and `case_manifest.json` integrity inventories under selected cases.
- **Hashing and verification:** calculate MD5, SHA1, SHA256, and SHA512; generate sidecar hashes for acquired evidence.
- **Image viewing:** mount supported images read-only for inspection.
- **Reports:** create case reports from collected outputs, notes, and iOS backup metadata summaries.
- **Updates:** check GitHub releases and download platform installers from inside the app.

## Downloads

Release builds are published on GitHub Releases and on the website.

- Linux AppImage: `amele-linux-x64.AppImage`
- Linux DEB: `amele-linux-x64.deb`
- Linux RPM: `amele-linux-x64.rpm`
- Arch Linux package: `amele-linux-x64.pkg.tar.zst`
- Windows MSI: `amele-windows-x64.msi`

Agent binaries:

```text
https://amele.noirlang.tr/amele-linux
https://amele.noirlang.tr/amele-win.exe
```

## Build Requirements

Module documentation:

- [Windows forensic module](docs/windows.md)
- [Linux forensic module](docs/linux.md)
- [Android forensic module](docs/android.md)
- [iOS forensic module](docs/ios.md)
- [Docker forensic module](docs/docker.md)

Install the Rust stable toolchain:

```bash
rustup toolchain install stable --component rustfmt
rustup default stable
```

Linux development packages:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev
```

Windows builds require the Microsoft Edge WebView2 Runtime on the target system.

## Build

Debug build:

```bash
cargo build --locked
```

Release build:

```bash
cargo build --release --locked
```

Run tests and checks:

```bash
cargo test --locked
cargo fmt --all -- --check
node --check ui/app.js
```

Build the Linux AppImage:

```bash
./scripts/build-appimage.sh
```

Build Linux DEB, RPM, and Arch packages:

```bash
./scripts/build-linux-packages.sh
```

## Run

Start the native desktop app:

```bash
cargo run -- ui
```

Run the release binary:

```bash
./target/release/amele ui
```

Open the browser-backed debug UI:

```bash
cargo run -- ui-browser
```

## Online Profile Security

Android and iOS tools are locked until a local profile is connected to an online `amele.noirlang.tr` account. The desktop app talks to the website API; MongoDB connection strings and database passwords must stay on the website/backend side and are not embedded in the app.

The backend checks the saved Bearer token against `/api/auth/session` and the current user profile before running mobile endpoints, so editing the local profile JSON is not enough to unlock mobile tools. The local token is stored under the user's Amele profile folder and is not returned by `/api/profiles`.

## Agents

Run the Linux agent on the target machine:

```bash
wget -O amele-linux https://amele.noirlang.tr/amele-linux
chmod +x amele-linux
./amele-linux
```

Download the Windows agent:

```text
https://amele.noirlang.tr/amele-win.exe
```

Connect to an agent from the app with IP address, port, and optional token.

## CI/CD / Automated Builds

This project uses **GitHub Actions** for automated building and packaging. 

To save runner resources, full builds and prereleases are only triggered when the commit message contains the `[build]` tag:

```bash
git commit -m "feat: add new feature [build]"
```

Commits without the `[build]` tag will be pushed to the `dev` branch but will not trigger build workflows.

**Manual Build:** You can also trigger the build workflow manually from the "Actions" tab in GitHub by clicking "Run workflow".

---

<div align="center">

<img src="ui/assets/logo/logo.png" alt="Amele Logo" width="120" />

# Amele Adli Bilişim Aracı (Forensic Tool)

*Windows, Linux, Android ve iOS için bütünleşik adli bilişim platformu. Disk, RAM, mantıksal edinim ve yedekleme analizi.*

[Web Sitesi](https://amele.noirlang.tr) | [Sürümler](https://github.com/noirlang/amele/releases) | [Katkıda Bulunma](CONTRIBUTING.md) | [Güvenlik](SECURITY.md) | [Linux Ajanı](https://github.com/noirlang/amele-linux) | [Windows Ajanı](https://github.com/noirlang/amele-win)

<img src="amele.gif" alt="Amele Forensic Tool Demo" width="700" />

</div>

## Genel Bakış

Amele, yetkili incelemeler için geliştirilmiş bir masaüstü adli edinim aracıdır. Disk imajı alma, bellek (RAM) edinimi, Android veri toplama, iOS backup normalizasyonu, hash doğrulama, vaka çıktısı yönetimi, imaj görüntüleme ve raporlama özelliklerini tek bir yerel uygulamada bir araya getirir.

Uygulama Linux ve Windows üzerinde gerçek bir masaüstü penceresi olarak çalışır.

## Özellikler

- **Yerel disk edinimi:** Yerel disklerden veya imaj dosyalarından ham (raw) disk imajları oluşturun.
- **Uzak disk edinimi:** Linux ve Windows ajanları (agents) aracılığıyla uzak disk imajları toplayın.
- **Yerel bellek (RAM) edinimi:** Linux üzerinde AVML ve Windows üzerinde WinPMEM ile RAM bellek kopyasını alın.
- **Uzak bellek (RAM) edinimi:** Ajanlar üzerinden RAM edinimini başlatın, duraklatın, sürdürün, durdurun, izleyin ve RAM dökümlerini indirin.
- **Android araçları:** ADB durumunu kontrol edin, cihazları listeleyin, mantıksal veri toplayın, dosya sistemi verisi toplayın, uçucu (volatile) verileri alın ve Android vaka çıktılarını analiz edin.
- **iOS araçları:** Şifresiz veya önceden decrypt edilmiş iTunes/Finder backup klasörlerini Backup2FS düzeninde gezilebilir dosya sistemine dönüştürün; her dosya için MD5/SHA1/SHA256 CSV log üretin.
- **Online profiller:** `amele.noirlang.tr` hesabını bağlayın, BDFL veya platform maintainer gibi rolleri profilde görün ve Android/iOS araçlarını doğrulanmış üyelikle kullanın.
- **Vaka yönetimi:** Edinimleri, notları, hash değerlerini, Android/iOS çıktılarını, raporları ve `case_manifest.json` bütünlük envanterlerini seçilen vakalar altında saklayın.
- **Hash hesaplama ve doğrulama:** MD5, SHA1, SHA256 ve SHA512 hesaplayın; elde edilen deliller için yan dosya (sidecar) hash dosyaları oluşturun.
- **İmaj görüntüleme:** Desteklenen imajları inceleme amacıyla salt okunur (read-only) olarak bağlayın (mount).
- **Raporlar:** Toplanan çıktılardan, notlardan ve iOS backup metadata özetlerinden vaka raporları oluşturun.
- **Güncellemeler:** Uygulama içerisinden GitHub sürümlerini kontrol edin ve platform yükleyicilerini indirin.

## İndirmeler

Kararlı sürümler GitHub Sürümleri (Releases) sayfasında ve web sitesinde yayınlanmaktadır.

- Linux AppImage: `amele-linux-x64.AppImage`
- Linux DEB: `amele-linux-x64.deb`
- Linux RPM: `amele-linux-x64.rpm`
- Arch Linux Paketi: `amele-linux-x64.pkg.tar.zst`
- Windows MSI: `amele-windows-x64.msi`

Ajan (Agent) ikili dosyaları:

```text
https://amele.noirlang.tr/amele-linux
https://amele.noirlang.tr/amele-win.exe
```

## Derleme Gereksinimleri

Modül dokümantasyonu:

- [Windows adli bilişim modülü](docs/windows.md)
- [Linux adli bilişim modülü](docs/linux.md)
- [Android adli bilişim modülü](docs/android.md)
- [iOS adli bilişim modülü](docs/ios.md)

Stabil Rust araç zincirini (toolchain) kurun:

```bash
rustup toolchain install stable --component rustfmt
rustup default stable
```

Linux geliştirme paketleri:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev
```

Windows derlemeleri, hedef sistemde Microsoft Edge WebView2 Çalışma Zamanı (Runtime) gerektirir.

## Derleme

Geliştirici (Debug) derlemesi:

```bash
cargo build --locked
```

Kararlı (Release) derlemesi:

```bash
cargo build --release --locked
```

Testleri ve kontrolleri çalıştırın:

```bash
cargo test --locked
cargo fmt --all -- --check
node --check ui/app.js
```

Linux AppImage derleme:

```bash
./scripts/build-appimage.sh
```

Linux DEB, RPM ve Arch paketlerini derleme:

```bash
./scripts/build-linux-packages.sh
```

## Çalıştırma

Yerel masaüstü uygulamasını başlatın:

```bash
cargo run -- ui
```

Kararlı ikili dosyayı çalıştırın:

```bash
./target/release/amele ui
```

Tarayıcı tabanlı hata ayıklama arayüzünü (debug UI) açın:

```bash
cargo run -- ui-browser
```

## Komut Satırı Kullanımı (CLI)

```bash
# Docker durumu, konteyner listesi ve loglar
amele docker-status
amele docker-list
amele docker-logs <container_id> 200

# Yerel konteyner delillerini (Overlay2 UpperDir drift, config, log) vakaya edin
amele docker-acquire <container_id> Case_Docker_001

# Uzak Linux Agent üzerinden Docker inceleme ve edinimi
amele docker-remote-status 192.168.1.100 8080 mysecrettoken
amele docker-remote-list 192.168.1.100 8080 mysecrettoken
amele docker-remote-acquire 192.168.1.100 8080 <container_id> Case_Remote_Docker mysecrettoken
```

## Online Profil Güvenliği

Android ve iOS araçları, yerel profil bir `amele.noirlang.tr` hesabına bağlanmadan kilitli kalır. Masaüstü uygulaması web sitesi API'siyle konuşur; MongoDB connection string ve veritabanı parolaları uygulamaya gömülmez, website/backend tarafında kalır.

Mobil endpointler çalışmadan önce backend kayıtlı Bearer token'ı `/api/auth/session` ve güncel kullanıcı profiliyle doğrular. Bu yüzden yerel profil JSON'unu elle düzenlemek Android/iOS araçlarını açmak için yeterli değildir. Yerel token kullanıcının Amele profil klasöründe tutulur ve `/api/profiles` cevabında döndürülmez.

## Ajanlar (Agents)

Hedef makinede Linux ajanını çalıştırın:

```bash
wget -O amele-linux https://amele.noirlang.tr/amele-linux
chmod +x amele-linux
./amele-linux
```

Windows ajanını indirin:

```text
https://amele.noirlang.tr/amele-win.exe
```

IP adresi, port ve isteğe bağlı token ile uygulama içerisinden ajana bağlanın.

## CI/CD / Otomatik Derlemeler

Bu proje, otomatik derleme ve paketleme işlemleri için **GitHub Actions** kullanmaktadır.

Sunucu (runner) kaynaklarını tasarruflu kullanmak amacıyla, tam sürüm derlemeleri ve ön sürümler (prerelease) yalnızca commit mesajı `[build]` etiketini içerdiğinde tetiklenir:

```bash
git commit -m "feat: add new feature [build]"
```

`[build]` etiketi içermeyen commit'ler `dev` dalına pushlanır ancak derleme iş akışlarını tetiklemez.

**Manuel Derleme:** GitHub'daki "Actions" sekmesinden "Run workflow" seçeneğine tıklayarak derleme iş akışını manuel olarak da tetikleyebilirsiniz.
