use crate::{AppError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportAudience {
    PrivateBackup,
    PublicShare,
    ProviderUpload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPolicy {
    pub audience: ExportAudience,
    pub include_private_metadata: bool,
    pub allow_overwrite: bool,
    pub allow_temporary_upload: bool,
}

impl ExportPolicy {
    pub fn private_backup(include_private_metadata: bool, allow_overwrite: bool) -> Self {
        Self {
            audience: ExportAudience::PrivateBackup,
            include_private_metadata,
            allow_overwrite,
            allow_temporary_upload: false,
        }
    }

    pub fn provider_upload(allow_overwrite: bool, allow_temporary_upload: bool) -> Self {
        Self {
            audience: ExportAudience::ProviderUpload,
            include_private_metadata: false,
            allow_overwrite,
            allow_temporary_upload,
        }
    }

    pub fn verify_private_metadata_export(self) -> Result<()> {
        if self.include_private_metadata && self.audience != ExportAudience::PrivateBackup {
            return Err(AppError::Message(
                "Private collector metadata can only be included in private backup exports"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub fn require_temporary_upload_confirmation(self, requested: bool) -> Result<()> {
        if requested && !self.allow_temporary_upload {
            return Err(AppError::Message(
                "Temporary upload requires explicit temporary upload confirmation".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExportAudience, ExportPolicy};

    #[test]
    fn public_or_provider_exports_cannot_include_private_metadata() {
        let policy = ExportPolicy {
            audience: ExportAudience::PublicShare,
            include_private_metadata: true,
            allow_overwrite: false,
            allow_temporary_upload: false,
        };

        assert!(policy.verify_private_metadata_export().is_err());
    }

    #[test]
    fn private_backup_can_include_private_metadata() {
        let policy = ExportPolicy::private_backup(true, false);

        assert!(policy.verify_private_metadata_export().is_ok());
    }
}
