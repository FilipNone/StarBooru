-- The automatic database backup schedule. NULL follows the environment variable.
ALTER TABLE server_settings ADD COLUMN backup_days INTEGER CHECK (backup_days >= 0);
ALTER TABLE server_settings ADD COLUMN backup_keep INTEGER CHECK (backup_keep >= 1);
