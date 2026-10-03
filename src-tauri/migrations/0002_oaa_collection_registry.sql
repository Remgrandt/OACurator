-- OAA collection membership exists independently of gallery membership.
CREATE TABLE IF NOT EXISTS collection_artwork (
  collection_id INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
  artwork_id INTEGER NOT NULL REFERENCES artwork(id) ON DELETE CASCADE,
  PRIMARY KEY (collection_id, artwork_id)
);
-- External associations are not globally unique artwork identities.
DROP INDEX IF EXISTS idx_external_link_provider_id;
CREATE INDEX idx_external_link_provider_id ON external_link(link_type, external_id)
  WHERE external_id IS NOT NULL;
