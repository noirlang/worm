//! HTTP API modüllerini ve ortak API yardımcılarını dışarı açar.
pub mod android;
pub mod desktop;
pub mod developer;
pub mod evidence;
mod helpers;
mod jobs;
mod mount;
pub mod ram;
mod ram_tools;
mod router;
pub mod settings;
mod state;
pub mod system;
pub mod update;
pub mod wireguard;

#[cfg(target_os = "linux")]
pub use helpers::elevated_helper_executable;
pub use helpers::{
    cleanup_helper_files, command_error_message, download_file_to_path, elevated_disk_list,
    helper_file_stem, helper_owner_gid, helper_owner_uid, process_is_root, read_helper_error,
    read_helper_json, read_helper_progress, run_elevated_helper_wait, sha256_file,
    spawn_elevated_helper, write_helper_control_state, write_json_file,
};
pub use jobs::{
    AcquisitionJob, NEXT_ACQUISITION_JOB_ID, acquisition_jobs, append_acquisition_log,
    create_acquisition_job, fail_acquisition_job_with_message, finish_acquisition_job_with_message,
    update_acquisition_message, update_acquisition_progress, update_acquisition_progress_message,
};
pub use mount::image_unmount_current;
#[cfg(target_os = "linux")]
pub use mount::{
    elevated_linux_mount_image_readonly, elevated_linux_unmount_image, linux_loop_mount_candidates,
    linux_mount_image_readonly, linux_mount_partitioned_image,
};
pub use router::route_api;
#[cfg(test)]
pub use state::test_case_base_dir;
pub use state::{
    EvidenceCaseState, ImageMountState, current_evidence_case, current_evidence_vault,
    current_image_mount, current_server_port, default_case_base_dir, default_case_name,
    evidence_subdir, evidence_vault_for_output, home_dir, report_evidence_vault,
    sanitize_case_name, sanitize_file_stem, set_current_evidence_case, set_server_port,
    wireguard_manager,
};
