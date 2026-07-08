//! Hata mesajlarını sınıflandırır ve kullanıcıya uygulanabilir çözüm önerileri üretir.
use std::path::Path;

#[derive(Debug, Clone, Copy)]
/// Kullanıcıya gösterilecek hata kodu, neden ve çözüm önerisini taşır.
pub struct ErrorAdvice {
    pub code: &'static str,
    pub detail: &'static str,
    pub suggestion: &'static str,
}

/// Ham hata mesajını bilinen hata sınıflarına ve çözüm önerilerine eşler.
pub fn classify_error(message: &str) -> ErrorAdvice {
    let lower = message.to_lowercase();

    if lower.contains("webview2") {
        return ErrorAdvice {
            code: "WEBVIEW2_RUNTIME",
            detail: "Windows native pencere WebView2 runtime olmadan acilamaz.",
            suggestion: "Microsoft Edge WebView2 Evergreen Runtime kurun, sonra Amele'u tekrar baslatin.",
        };
    }

    if lower.contains("vcruntime") || lower.contains("msvcp") {
        return ErrorAdvice {
            code: "WINDOWS_RUNTIME_DLL",
            detail: "Windows Visual C++ runtime DLL dosyasi sistemde bulunamadi.",
            suggestion: "Guncel Amele paketini kullanin. Eski paketlerde Microsoft Visual C++ Redistributable kurulumu gerekebilir.",
        };
    }

    if lower.contains("webkitnetworkprocess") || lower.contains("webkit helper") {
        return ErrorAdvice {
            code: "WEBKIT_HELPER_MISSING",
            detail: "Linux WebKit alt sureci bulunamadigi icin native pencere baslatilamiyor.",
            suggestion: "Dağıtım WebKitGTK paketlerini kurun veya WebKitNetworkProcess iceren AppImage/.deb/.rpm paketini kullanin.",
        };
    }

    if lower.contains("glibc") || lower.contains("glibc_") || lower.contains("libm.so.6") {
        return ErrorAdvice {
            code: "LINUX_RUNTIME_INCOMPATIBLE",
            detail: "Linux paketi sistemdeki glibc/runtime surumuyle uyumlu degil.",
            suggestion: "Daha eski glibc ile derlenmis Amele paketini, .deb/.rpm paketini veya desteklenen dagitim surumunu kullanin.",
        };
    }

    if lower.contains("cannot mount appimage") || lower.contains("fuse") {
        return ErrorAdvice {
            code: "APPIMAGE_FUSE_MISSING",
            detail: "AppImage dosyasi FUSE destegi olmadigi icin baglanamadi.",
            suggestion: "FUSE paketini kurun veya APPIMAGE_EXTRACT_AND_RUN=1 ile calistirmayi deneyin.",
        };
    }

    if lower.contains("gtk")
        && (lower.contains("display") || lower.contains("cannot open") || lower.contains("ekran"))
    {
        return ErrorAdvice {
            code: "DISPLAY_UNAVAILABLE",
            detail: "GTK ekrana baglanamadi; grafik oturum veya DISPLAY/WAYLAND ayari uygun degil.",
            suggestion: "Uygulamayi masaustu oturumunda baslatin. SSH/terminal uzerindeyseniz X11/Wayland erisimini kontrol edin.",
        };
    }

    if lower.contains("uac")
        || lower.contains("runas")
        || lower.contains("pkexec")
        || lower.contains("sudo")
        || lower.contains("askpass")
        || lower.contains("polkit")
        || lower.contains("yetki yükseltme")
        || lower.contains("yetki yukseltme")
        || lower.contains("root yetkisi")
        || lower.contains("administrator privileges")
        || lower.contains("requires administrator")
    {
        return ErrorAdvice {
            code: "ELEVATION_FAILED",
            detail: "Islem root/administrator yetkisi gerektiriyor ancak yetki onayi tamamlanamadi.",
            suggestion: "Linux'ta sudo/pkexec parola penceresini onaylayin; pencere acilmiyorsa polkit agent veya zenity/kdialog/ssh-askpass kurun. Windows'ta UAC penceresini onaylayin veya Amele'u yonetici olarak baslatin.",
        };
    }

    if lower.contains("permission denied")
        || lower.contains("access denied")
        || lower.contains("erisim engellendi")
        || lower.contains("erişim engellendi")
        || lower.contains("os error 13")
    {
        return ErrorAdvice {
            code: "PERMISSION_DENIED",
            detail: "Islem icin gereken disk, RAM veya sistem dosyasi yetkisi yok.",
            suggestion: "Linux'ta pkexec/sudo yetkilendirmesini onaylayin. Windows'ta uygulamayi yonetici olarak baslatin veya UAC iznini verin.",
        };
    }

    if lower.contains("no space left")
        || lower.contains("yetersiz bos")
        || lower.contains("yetersiz boş")
        || lower.contains("insufficient space")
        || lower.contains("os error 28")
    {
        return ErrorAdvice {
            code: "INSUFFICIENT_SPACE",
            detail: "Cikti veya gecici dosyalar icin yeterli bos alan yok.",
            suggestion: "Vaka klasorunu daha genis bir diske tasiyin. Linux'ta /tmp tmpfs ise RAM dump sirasinda /var/tmp veya disk uzerindeki bir klasor kullanin.",
        };
    }

    if lower.contains("unable to create memory snapshot")
        || lower.contains("/proc/kcore")
        || lower.contains("write block failed")
    {
        return ErrorAdvice {
            code: "RAM_SNAPSHOT_FAILED",
            detail: "RAM imaji uretilirken cekirdek bellegi veya gecici snapshot yazimi basarisiz oldu.",
            suggestion: "Root yetkisini, kernel lockdown durumunu, /tmp kapasitesini ve cikti diskindeki bos alani kontrol edin.",
        };
    }

    if lower.contains("symbol_table_name")
        || lower.contains("layer_name")
        || lower.contains("unsatisfied requirement")
        || lower.contains("translation layer")
        || lower.contains("kernel banner")
        || lower.contains("volatility3")
    {
        return ErrorAdvice {
            code: "VOLATILITY_SYMBOLS_REQUIRED",
            detail: "Volatility3 bellek imaji icin gereken profil/sembol eslesmesini kuramadi.",
            suggestion: "Dogru isletim sistemi profilini secin. Linux RAM imajlari icin kernel banner ile uyumlu ISF sembol dosyasini .symbols klasorune ekleyin.",
        };
    }

    if lower.contains("wrong fs type")
        || lower.contains("bad superblock")
        || lower.contains("mount failed")
        || lower.contains("losetup failed")
        || lower.contains("mounted image has no drive letter")
        || lower.contains("raw dd/img")
    {
        return ErrorAdvice {
            code: "IMAGE_MOUNT_FAILED",
            detail: "Disk imaji salt-okunur baglanamadi; imaj bolum tablosu, dosya sistemi veya yetki sorunu icerebilir.",
            suggestion: "Imajin tamamlandigini/hash dogrulamasini kontrol edin. Linux'ta mount/losetup icin root yetkisi ve gerekli dosya sistemi paketleri gerekir.",
        };
    }

    if lower.contains("adb")
        && (lower.contains("unauthorized")
            || lower.contains("offline")
            || lower.contains("no devices")
            || lower.contains("device not found")
            || lower.contains("more than one device"))
    {
        return ErrorAdvice {
            code: "ADB_DEVICE_NOT_READY",
            detail: "Android cihaz ADB tarafinda hazir degil veya hedef cihaz net secilemedi.",
            suggestion: "USB hata ayiklamayi acin, cihazdaki RSA onayini kabul edin ve tek hedef cihazi secerek tekrar deneyin.",
        };
    }

    if lower.contains("avml")
        || lower.contains("winpmem")
        || lower.contains("command not found")
        || lower.contains("not installed")
        || lower.contains("arac bulunamadi")
        || lower.contains("araç bulunamadı")
    {
        return ErrorAdvice {
            code: "ACQUISITION_TOOL_MISSING",
            detail: "Gerekli edinim araci bulunamadi veya calistirilamadi.",
            suggestion: "Arac kontrolunu tekrar calistirin ve indir/kur butonuyla AVML veya WinPMEM'i Amele'un bekledigi konuma kurun.",
        };
    }

    if lower.contains("connection refused")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("baglanti")
        || lower.contains("bağlantı")
        || lower.contains("connection reset")
    {
        return ErrorAdvice {
            code: "CONNECTION_FAILED",
            detail: "Agent veya yerel backend ile ag baglantisi kurulamadi ya da islem zaman asimina ugradi.",
            suggestion: "IP/port/token bilgisini, firewall kurallarini, agent'in calistigini ve ayni ag/VPN erisimini kontrol edin.",
        };
    }

    if (lower.contains("hash") || lower.contains("sha256") || lower.contains("md5"))
        && !lower.contains("verified")
        && !lower.contains("winget")
        && !lower.contains("installer")
        && !lower.contains("download")
    {
        return ErrorAdvice {
            code: "HASH_CALCULATION_FAILED",
            detail: "Dosya hash hesabi tamamlanamadi.",
            suggestion: "Dosyanin mevcut ve okunabilir oldugunu, imaj alma isleminin bitmis oldugunu ve diskin uyku/ayrilma durumunda olmadigini kontrol edin.",
        };
    }

    if lower.contains("address already in use") || lower.contains("only one usage") {
        return ErrorAdvice {
            code: "LOCAL_PORT_BUSY",
            detail: "Yerel UI backend portu acilamadi.",
            suggestion: "Arka planda kalmis Amele sureclerini kapatin ve uygulamayi tekrar baslatin.",
        };
    }

    if lower.contains("not found")
        || lower.contains("no such file")
        || lower.contains("bulunamadi")
        || lower.contains("bulunamadı")
    {
        return ErrorAdvice {
            code: "FILE_NOT_FOUND",
            detail: "Gerekli dosya, arac veya paketlenen UI varligi bulunamadi.",
            suggestion: "Secilen yolu ve kurulum paketini kontrol edin. Paket icinden calisiyorsaniz uygulamayi yeniden kurun.",
        };
    }

    if lower.contains("json") || lower.contains("parse") || lower.contains("invalid") {
        return ErrorAdvice {
            code: "INVALID_DATA",
            detail: "Beklenen veri formati okunamadi veya bozuk geldi.",
            suggestion: "Islemi tekrar deneyin. Agent ve uygulama surumlerinin ayni oldugundan emin olun.",
        };
    }

    ErrorAdvice {
        code: "UNEXPECTED_ERROR",
        detail: "Islem beklenmeyen bir hata ile durdu.",
        suggestion: "Ayrinti mesajini ve log dosyasini kontrol edin; ayni adimi tekrar calistirmadan once yetki, disk alani ve secilen yolları dogrulayin.",
    }
}

/// Ham hata metnini Kod/Neden/Cozum satirlariyla zenginlestirir.
pub fn error_with_advice(message: &str) -> String {
    let message = message.trim();
    if message.is_empty() {
        return String::new();
    }
    if has_structured_advice(message) {
        return message.to_string();
    }
    let advice = classify_error(message);
    format!(
        "{message}\nKod: {}\nNeden: {}\nCozum: {}",
        advice.code, advice.detail, advice.suggestion
    )
}

/// Hata metninde zaten kullaniciya donen yapilandirilmis oneriler var mi kontrol eder.
pub fn has_structured_advice(message: &str) -> bool {
    message.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("Kod:")
            || trimmed.starts_with("Code:")
            || trimmed.starts_with("Neden:")
            || trimmed.starts_with("Reason:")
            || trimmed.starts_with("Cozum:")
            || trimmed.starts_with("Çözüm:")
            || trimmed.starts_with("Suggestion:")
    })
}

/// Uygulama açılış hatasını ayrıntılı kullanıcı mesajına dönüştürür.
pub fn startup_error(stage: &str, message: &str) -> String {
    let advice = classify_error(message);
    format!(
        "{stage}\n\nAyrinti: {message}\nKod: {}\nNeden: {}\nCozum: {}",
        advice.code, advice.detail, advice.suggestion
    )
}

/// UI assetleri bulunamadığında açıklayıcı başlangıç hatası üretir.
pub fn ui_assets_missing(root: &Path) -> String {
    startup_error(
        "Arayuz dosyalari bulunamadi.",
        &format!(
            "index.html beklenen klasorde yok: {}",
            root.join("index.html").display()
        ),
    )
}

/// panic payload içeriğini okunabilir stringe dönüştürür.
pub fn panic_payload(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(value) = payload.downcast_ref::<&str>() {
        return (*value).to_string();
    }
    if let Some(value) = payload.downcast_ref::<String>() {
        return value.clone();
    }
    "panic payload could not be decoded".to_string()
}

#[cfg(test)]
mod tests {
    use super::classify_error;

    #[test]
    fn classifies_permission_errors() {
        assert_eq!(
            classify_error("Disk access error: Access denied (os error 13)").code,
            "PERMISSION_DENIED"
        );
    }

    #[test]
    fn classifies_elevation_errors() {
        assert_eq!(
            classify_error("pkexec failed: No authentication agent found").code,
            "ELEVATION_FAILED"
        );
        assert_eq!(
            classify_error("Windows UAC penceresi kullanıcı tarafından iptal edildi").code,
            "ELEVATION_FAILED"
        );
    }

    #[test]
    fn classifies_webview2_errors() {
        assert_eq!(
            classify_error("WebView2 view could not be created").code,
            "WEBVIEW2_RUNTIME"
        );
    }

    #[test]
    fn classifies_volatility_symbol_errors() {
        assert_eq!(
            classify_error("Unsatisfied requirement plugins.PsList.kernel.symbol_table_name").code,
            "VOLATILITY_SYMBOLS_REQUIRED"
        );
    }

    #[test]
    fn classifies_appimage_fuse_errors() {
        assert_eq!(
            classify_error("Cannot mount AppImage, please check your FUSE setup").code,
            "APPIMAGE_FUSE_MISSING"
        );
    }

    #[test]
    fn classifies_adb_device_errors() {
        assert_eq!(
            classify_error("adb: device unauthorized").code,
            "ADB_DEVICE_NOT_READY"
        );
    }
}
