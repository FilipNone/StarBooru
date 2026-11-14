-- Whether AI tags the post an import makes. NULL leaves it to the server's setting.
ALTER TABLE import_jobs ADD COLUMN autotag BOOLEAN;
