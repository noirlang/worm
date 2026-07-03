//! Android edinim, profil ve analiz alt modüllerini tek noktadan dışarı açar.
mod adb;
mod app_catalog;
mod capability;
mod errors;
mod extractors;
mod filesystem;
mod logical;
mod manifest;
mod orchestrator;
mod profile;
mod ram;
mod remote;
mod session;

pub use adb::{AdbInstallResult, AdbStatus, AndroidDevice, adb_status, install_adb, list_devices};
pub use capability::{
    AndroidCapabilityCheck, AndroidCapabilityLevel, AndroidCapabilityReport,
    build_android_capability_report,
};
pub use errors::explain_android_error;
pub use extractors::{
    AndroidAcquisitionProfile, AndroidExtractorStep, FULL_LOGICAL_STEPS, logical_steps_for_profile,
};
pub use filesystem::{FilesystemAcquisitionResult, filesystem_acquisition};
pub use logical::{
    AcquisitionItem, LogicalAcquisitionResult, logical_acquisition,
    logical_acquisition_with_profile,
};
pub use manifest::{AndroidAcquisitionManifest, AndroidManifestArtifact, write_android_manifest};
pub use orchestrator::{
    AndroidOrchestratedAcquisitionResult, AndroidOrchestratedFilesystemResult,
    AndroidOrchestratedRamResult, orchestrated_acquisition, orchestrated_filesystem_acquisition,
    orchestrated_ram_acquisition,
};
pub use profile::{AndroidDeviceProfile, detect_device_profile};
pub use ram::{
    AndroidRamAcquisitionResult, AndroidRamMode, ram_acquisition, ram_acquisition_with_mode,
};
pub use remote::{
    LemonPreflight, RemoteAndroidEndpoint, RemoteConnectResult, RemoteEndpointKind,
    connect_remote_endpoint, disconnect_remote_endpoint, lemon_preflight,
};
pub use session::{AndroidSession, AndroidTransport, AndroidTransportKind, build_android_session};
