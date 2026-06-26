# Exporting For SNIKT.com

SNIKT.com does not currently provide a bulk upload file format for OA Curator to write.

OA Curator can open a SNIKT.com upload-prefill URL for a selected Artwork. That workflow is useful for creating a new SNIKT.com piece with prefilled metadata, but it is not a full synchronization workflow.

SNIKT.com still requires you to manually select the image file on the upload page. The metadata fields are hidden until an image is selected; after you choose the image, SNIKT.com reads the upload-prefill URL and fills the supported fields.

## Private Fields

The **SNIKT export** button opens a SNIKT.com upload URL that includes supported OA Curator metadata for the selected Artwork. This can include the Artwork's estimated value. Estimated value is a private collector field inside OA Curator, but it is intentionally included in this SNIKT.com upload-prefill workflow so the SNIKT.com form can receive it.

Only use **SNIKT export** when you are ready to send the prefilled values to SNIKT.com. The values are placed in the browser URL for the SNIKT.com upload page and may also appear in normal browser history for that site.
