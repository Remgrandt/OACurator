//! Import portable base values without applying provider-specific editing rules.
use super::*;

impl Catalog {
    pub(crate) fn link_oaa_artwork_to_collection(
        &self,
        collection_id: i64,
        artwork_id: i64,
    ) -> Result<()> {
        self.lock()?.execute(
            "INSERT OR IGNORE INTO collection_artwork VALUES (?1,?2)",
            params![collection_id, artwork_id],
        )?;
        Ok(())
    }

    pub(crate) fn create_oaa_artwork(
        &self,
        collection_id: i64,
        title: &str,
    ) -> Result<ArtworkSummary> {
        let collection = self.collection_summary(collection_id)?;
        let now = Utc::now().to_rfc3339();
        let id = {
            let conn = self.lock()?;
            let canonical_id = next_canonical_id_locked(&conn, &collection.manifest_path)?;
            let path = collection
                .manifest_path
                .parent()
                .ok_or_else(|| AppError::Message("Collection has no folder".into()))?
                .join("artworks")
                .join(&canonical_id)
                .join(".oaartwork");
            conn.execute("INSERT INTO artwork (canonical_id,artwork_stable_id,title,source_folder,source_context,artwork_manifest_path,created_at,updated_at) VALUES (?1,?1,?2,?3,'OAA import',?3,?4,?4)",params![canonical_id,title,path.to_string_lossy(),now])?;
            conn.last_insert_rowid()
        };
        self.link_oaa_artwork_to_collection(collection_id, id)?;
        self.artwork_summary(id)
    }

    pub(crate) fn finish_oaa_import(&self, collection_id: i64) -> Result<()> {
        self.rewrite_collection_manifest(collection_id)
    }

    pub(crate) fn import_oaa_gallery_links(
        &self,
        id: i64,
        links: &[ExternalLinkManifest],
    ) -> Result<()> {
        self.save_oaa_links("gallery", id, links)?;
        self.lock()?.execute("UPDATE gallery SET caf_gallery_room_id=?2,snikt_gallery_id=?3,raremarq_gallery_id=?4,snikt_gallery_inherits_collection=0 WHERE id=?1", params![id, provider_id(links,"com.comicartfans"),provider_id(links,"com.snikt"),provider_id(links,"com.raremarq")])?;
        self.rewrite_gallery_manifest(id)
    }

    pub(crate) fn save_oaa_links(
        &self,
        kind: &str,
        id: i64,
        links: &[ExternalLinkManifest],
    ) -> Result<()> {
        self.save_oaa_extension_block(
            &format!("{kind}_external_links"),
            id,
            "app.oa-curator",
            &serde_json::json!({"links":links}),
        )?;
        if kind == "artwork" {
            let conn = self.lock()?;
            let mut seen = BTreeSet::new();
            for link in links {
                let Some(provider) = oac_artwork_identity_link_type_for_provider(&link.provider)
                else {
                    continue;
                };
                if !seen.insert(provider) {
                    continue;
                }
                conn.execute("INSERT INTO external_link (artwork_id,link_type,external_id,url,extensions_json) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(artwork_id,link_type) DO UPDATE SET external_id=excluded.external_id,url=excluded.url,extensions_json=excluded.extensions_json",params![id,provider,link.id,link.url,serde_json::to_string(&link.extensions)?])?;
            }
        }
        Ok(())
    }

    pub(crate) fn retained_oaa_links(
        &self,
        kind: &str,
        id: i64,
        mut links: Vec<ExternalLinkManifest>,
    ) -> Result<Vec<ExternalLinkManifest>> {
        for (_, block) in self.oaa_extension_blocks(&format!("{kind}_external_links"), id)? {
            if let Some(saved) = block.get("links") {
                let mut known = BTreeSet::new();
                for link in serde_json::from_value::<Vec<ExternalLinkManifest>>(saved.clone())? {
                    if matches!(kind, "artwork" | "collection" | "gallery")
                        && matches!(
                            link.provider.as_str(),
                            "com.comicartfans" | "com.snikt" | "com.raremarq"
                        )
                        && known.insert(link.provider.clone())
                    {
                        if let Some(current) = links.iter_mut().find(|current| {
                            current.provider == link.provider && current.id == link.id
                        }) {
                            if kind != "artwork" {
                                *current = link;
                            }
                        }
                    } else {
                        links.push(link);
                    }
                }
            }
        }
        Ok(links)
    }

    pub(crate) fn save_oaa_file(
        &self,
        kind: &str,
        id: i64,
        file: &crate::manifest::ArtworkFileManifest,
    ) -> Result<()> {
        self.save_oaa_extension_block(
            &format!("{kind}_oaa"),
            id,
            "app.oa-curator",
            &serde_json::json!({"file": file}),
        )
    }

    pub(crate) fn restore_oaa_file(
        &self,
        kind: &str,
        id: i64,
        file: &mut crate::manifest::ArtworkFileManifest,
    ) -> Result<()> {
        for (_, block) in self.oaa_extension_blocks(&format!("{kind}_oaa"), id)? {
            if let Some(saved) = block.get("file") {
                let saved: crate::manifest::ArtworkFileManifest =
                    serde_json::from_value(saved.clone())?;
                file.file_kind = saved.file_kind;
                file.format = saved.format;
                file.media_type = saved.media_type;
                file.external_links = saved.external_links;
                for (key, value) in saved.extensions {
                    file.extensions.entry(key).or_insert(value);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn import_oaa_artwork_metadata(
        &self,
        artwork_id: i64,
        manifest: &ArtworkManifest,
    ) -> Result<()> {
        let public = manifest.public_metadata.as_ref();
        let private = manifest.private_metadata.as_ref();
        let caf = public.and_then(|m| m.extensions.get("com.comicartfans"));
        let caf_string = |key: &str| {
            caf.and_then(|v| v.get(key))
                .and_then(serde_json::Value::as_str)
        };
        let publication = match public.and_then(|m| m.publication_status.as_deref()) {
            Some("published_art") => "1",
            Some("unpublished_art") => "2",
            _ => "",
        };
        {
            let conn = self.lock()?;
            conn.execute("UPDATE artwork SET title=?2,description=?3,for_sale_status=?4,media=?5,format=?6,media_type_id=?7,art_type_id=?8,publication_status_id=?9,active=?10,caf_csv_image_link=?11,caf_csv_added_to_caf=?12,updated_at=?13 WHERE id=?1",params![artwork_id,manifest.title,public.and_then(|m|m.description.as_deref()),public.and_then(|m|m.for_sale_status.as_deref()).unwrap_or(""),public.and_then(|m|m.media.as_deref()),public.and_then(|m|m.artwork_type.as_deref()),public.and_then(|m|m.media.as_deref()).and_then(media_type_id_for_label).unwrap_or("7"),public.and_then(|m|m.artwork_type.as_deref()).and_then(art_type_id_for_label).unwrap_or("3"),publication,public.and_then(|m|m.is_public).unwrap_or(false),caf_string("csv_image_link"),caf_string("csv_added_to_caf"),Utc::now().to_rfc3339()])?;
            conn.execute("UPDATE artwork SET illustration_exchange=?2,ix_for_sale=?3,snikt_csv_created_date=?4 WHERE id=?1", params![artwork_id,caf.and_then(|v|v.get("illustration_exchange")).and_then(serde_json::Value::as_bool).unwrap_or(false),caf.and_then(|v|v.get("ix_for_sale")).and_then(serde_json::Value::as_bool).unwrap_or(false),public.and_then(|m|m.extensions.get("com.snikt")).and_then(|v|v.get("metadata")).and_then(|v|v.get("csv_created_date")).and_then(serde_json::Value::as_str)])?;
            if let Some(url) = app_extension_string(&manifest.extensions, "generic_url") {
                upsert_external_link_locked(&conn, artwork_id, "generic", None, &url)?;
            }
            if let Some(media) = public.and_then(|m| m.media.as_deref()) {
                conn.execute(
                    "INSERT OR IGNORE INTO term_media(value) VALUES (?1)",
                    params![media],
                )?;
            }
            if let Some(format) = public.and_then(|m| m.artwork_type.as_deref()) {
                conn.execute(
                    "INSERT OR IGNORE INTO term_format(value) VALUES (?1)",
                    params![format],
                )?;
            }
            conn.execute("INSERT INTO private_metadata (artwork_id,purchase_price,estimated_value,purchase_date,provenance,personal_notes) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(artwork_id) DO UPDATE SET purchase_price=excluded.purchase_price,estimated_value=excluded.estimated_value,purchase_date=excluded.purchase_date,provenance=excluded.provenance,personal_notes=excluded.personal_notes",params![artwork_id,private.and_then(|m|m.purchase_price.as_deref()),private.and_then(|m|m.estimated_value.as_deref()),private.and_then(|m|m.purchase_date.as_deref()),private.and_then(|m|m.provenance.as_deref()),private.and_then(|m|m.personal_notes.as_deref())])?;
            conn.execute(
                "DELETE FROM artwork_artist WHERE artwork_id=?1",
                params![artwork_id],
            )?;
            for (order, credit) in public
                .into_iter()
                .flat_map(|m| &m.artist_credits)
                .enumerate()
            {
                let name = credit.display_name.clone().unwrap_or_else(|| {
                    [credit.first_name.as_deref(), credit.last_name.as_deref()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ")
                });
                conn.execute(
                    "INSERT OR IGNORE INTO artist(name) VALUES (?1)",
                    params![name],
                )?;
                let artist_id: i64 =
                    conn.query_row("SELECT id FROM artist WHERE name=?1", params![name], |r| {
                        r.get(0)
                    })?;
                conn.execute("INSERT OR REPLACE INTO artwork_artist (artwork_id,artist_id,role,sort_order,first_name,last_name,role_id) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![artwork_id,artist_id,credit.role,order as i64,credit.first_name,credit.last_name,credit.role.as_deref().and_then(artist_role_id_for_label)])?;
            }
        }
        // Known provider fields remain optional; opaque provider JSON cannot reject base metadata.
        if let Some(metadata) = public
            .and_then(|m| m.extensions.get("com.snikt"))
            .and_then(|v| v.get("metadata"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        {
            let conn = self.lock()?;
            save_snikt_metadata_locked(&conn, artwork_id, &metadata)?;
        }
        self.save_oaa_links("artwork", artwork_id, &manifest.external_links)?;
        Ok(())
    }
}
