include!("legacy/part01_types.rs");
include!("legacy/part02_inspection.rs");
include!("legacy/part03_plan.rs");
include!("legacy/part04_profile_plan.rs");
include!("legacy/part05_profile_support.rs");
include!("legacy/part06_profile_apply.rs");
include!("legacy/part07_commands.rs");
include!("legacy/part08_install.rs");
include!("legacy/part09_recovery.rs");
include!("legacy/part09_state.rs");
include!("legacy/part09_shared.rs");
include!("legacy/part10_io.rs");
include!("legacy/part14_cloud_cache.rs");
include!("legacy/part11_cli.rs");
include!("legacy/part15_campaign_routes.rs");
include!("legacy/part12_discovery.rs");
include!("legacy/part13_legacy_migration.rs");

#[cfg(test)]
mod tests_a {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};
    include!("legacy/tests_a.rs");
}

#[cfg(test)]
mod tests_b {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};
    include!("legacy/tests_b.rs");
}

#[cfg(test)]
mod tests_c {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};
    include!("legacy/tests_c.rs");
}
