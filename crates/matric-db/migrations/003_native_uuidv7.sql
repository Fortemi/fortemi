-- PostgreSQL 18 native UUIDv7 for shared OAuth/auth tables (#397)
--
-- Switches gen_random_uuid() (v4) defaults to native uuidv7() on shared tables.

ALTER TABLE oauth_client ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE oauth_token ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE api_key ALTER COLUMN id SET DEFAULT uuidv7();
