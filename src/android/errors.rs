//! Android/ADB hatalarını kullanıcıya anlaşılır açıklamalara çevirir.
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Kullanıcıya gösterilecek kodlu Android hata açıklamasıdır.
pub struct AndroidErrorInfo {
    pub code: &'static str,
    pub title: &'static str,
    pub cause: &'static str,
    pub solution: &'static str,
    pub detail: String,
}

/// Ham ADB/Android hatasını kullanıcıya ne yapılacağını söyleyen mesaja çevirir.
pub fn explain_android_error(error: impl AsRef<str>) -> String {
    let info = classify_android_error(error);
    format!(
        "{}\nKod: {}\nNeden: {}\nÇözüm: {}\nTeknik detay: {}",
        info.title, info.code, info.cause, info.solution, info.detail
    )
}

/// Ham hata metnini standart Android hata koduna indirger.
pub fn classify_android_error(error: impl AsRef<str>) -> AndroidErrorInfo {
    let raw = error.as_ref().trim();
    if raw.is_empty() {
        return AndroidErrorInfo {
            code: "ANDROID_UNKNOWN",
            title: "Android işlemi başarısız oldu",
            cause: "Komut hata ayrıntısı döndürmedi.",
            solution: "Cihaz bağlantısını yenileyin ve işlemi tekrar çalıştırın.",
            detail: "Ayrıntı yok".to_string(),
        };
    }

    let lower = raw.to_ascii_lowercase();
    let (code, title, cause, solution) = if contains_any(
        &lower,
        &["adb bulunamadi", "adb bulunamadı"],
    ) || (lower.contains("adb")
        && contains_any(&lower, &["no such file", "not found"]))
    {
        (
            "ADB_NOT_FOUND",
            "ADB bulunamadı",
            "Android Platform Tools kurulu değil veya adb PATH içinde değil.",
            "Platform Tools kurun ya da adb yolunu PATH içine ekleyin.",
        )
    } else if lower.contains("unauthorized") {
        (
            "ADB_UNAUTHORIZED",
            "Cihaz ADB için yetkilendirilmemiş",
            "Telefon USB hata ayıklama iznini onaylamamış.",
            "Telefon ekranındaki RSA/USB hata ayıklama onayını kabul edin ve tekrar deneyin.",
        )
    } else if contains_any(
        &lower,
        &[
            "no devices/emulators found",
            "no devices",
            "no device found",
            "device not found",
            "cihaz bulunamadi",
            "cihaz bulunamadı",
            "file_not_found",
        ],
    ) {
        (
            "ADB_DEVICE_NOT_FOUND",
            "Android cihaz bulunamadı",
            "ADB hedef cihaza erişemedi veya seçilen serial artık geçerli değil.",
            "USB kablosunu, cihaz seçimini, adb devices çıktısını ve USB hata ayıklamayı kontrol edin.",
        )
    } else if lower.contains("offline") {
        (
            "ADB_DEVICE_OFFLINE",
            "Android cihaz offline görünüyor",
            "ADB daemon cihazla sağlıklı oturum kuramadı.",
            "USB bağlantısını yenileyin, adb kill-server/start-server çalıştırın ve yetkiyi tekrar onaylayın.",
        )
    } else if lower.contains("more than one device") || lower.contains("more than one emulator") {
        (
            "ADB_MULTIPLE_DEVICES",
            "Birden fazla Android hedefi var",
            "Komut hangi cihaza çalışacağını belirleyemedi.",
            "Uygulamadaki cihaz listesinden tek hedef seçin veya diğer cihazları ayırın.",
        )
    } else if contains_any(
        &lower,
        &[
            "su: not found",
            "inaccessible or not found",
            "adbd cannot run as root",
            "root profili secildi",
            "root profili seçildi",
            "root yetkisi alinamadi",
            "root yetkisi alınamadı",
        ],
    ) {
        (
            "ANDROID_ROOT_REQUIRED",
            "Root erişimi kullanılamıyor",
            "Seçilen Android işlemi root veya adbd root gerektiriyor.",
            "Root gerektirmeyen logical/volatile modu seçin veya root erişimi olan test cihazıyla deneyin.",
        )
    } else if contains_any(&lower, &["permission denied", "operation not permitted"]) {
        (
            "ANDROID_PERMISSION_DENIED",
            "Android işlem izni reddedildi",
            "Cihaz güvenlik politikası bu dosya veya bellek alanını engelledi.",
            "Root gerekip gerekmediğini, cihaz kilidini ve Android sürüm kısıtlarını kontrol edin.",
        )
    } else if contains_any(&lower, &["read-only file system", "not permitted"]) {
        (
            "ANDROID_FILESYSTEM_RESTRICTED",
            "Dosya sistemi işlemi kısıtlandı",
            "Hedef yol salt okunur veya Android güvenlik politikası tarafından kapalı.",
            "Root modu, hedef yol ve non-root filesystem seçeneğini kontrol edin.",
        )
    } else if lower.contains("closed") || lower.contains("connection reset") {
        (
            "ADB_CONNECTION_LOST",
            "ADB bağlantısı işlem sırasında koptu",
            "Kablo, cihaz ekran kilidi veya ADB servisi uzun işlem sırasında kesildi.",
            "Cihazı uyanık tutun, kabloyu değiştirin ve işlemi tekrar başlatın.",
        )
    } else if lower.contains("timeout") || lower.contains("zaman asimina") {
        (
            "ADB_TIMEOUT",
            "ADB komutu zaman aşımına uğradı",
            "Cihaz komuta beklenen sürede cevap vermedi.",
            "Cihaz kilidini açın, ağır işlemleri durdurun ve gerekirse ADB servisini yeniden başlatın.",
        )
    } else if lower.contains("bugreport") && contains_any(&lower, &["failed", "error"]) {
        (
            "ANDROID_BUGREPORT_FAILED",
            "Android bugreport alınamadı",
            "Cihaz bugreport komutunu tamamlamadı veya yeterli alan/izin yok.",
            "Cihaz depolama alanını, ekran kilidini ve USB hata ayıklama yetkisini kontrol edin.",
        )
    } else {
        (
            "ANDROID_COMMAND_FAILED",
            "Android işlemi başarısız oldu",
            "ADB veya cihaz komutu beklenmeyen hata döndürdü.",
            "Canlı konsoldaki teknik detayı kontrol edin ve aynı işlemi tekrar deneyin.",
        )
    };

    AndroidErrorInfo {
        code,
        title,
        cause,
        solution,
        detail: raw.to_string(),
    }
}

/// Hata metninde anahtar kelimelerden herhangi biri geçiyor mu diye kontrol eder.
fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_unauthorized_devices() {
        let message = explain_android_error("device unauthorized");
        assert!(message.contains("ADB_UNAUTHORIZED"));
    }

    #[test]
    fn explains_missing_adb() {
        let message = explain_android_error("ADB bulunamadi");
        assert!(message.contains("ADB_NOT_FOUND"));
    }

    #[test]
    fn explains_missing_device() {
        let message = explain_android_error("Cihaz bulunamadı");
        assert!(message.contains("ADB_DEVICE_NOT_FOUND"));
    }

    #[test]
    fn explains_permission_denied() {
        let message = explain_android_error("permission denied");
        assert!(message.contains("ANDROID_PERMISSION_DENIED"));
    }

    #[test]
    fn classifies_root_required_errors() {
        let info = classify_android_error("adbd cannot run as root in production builds");
        assert_eq!(info.code, "ANDROID_ROOT_REQUIRED");
    }
}
