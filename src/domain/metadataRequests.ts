import { ART_TYPE_OPTIONS, MEDIA_TYPE_OPTIONS } from "./constants";
import { blankToNull, labelForId } from "./formatters";
import type { DetailForm, MetadataSaveRequest } from "./types";

export function metadataRequestForForm(
  artworkId: number,
  detailForm: DetailForm,
): MetadataSaveRequest {
  return {
    artwork_id: artworkId,
    title: detailForm.title,
    description: blankToNull(detailForm.description),
    for_sale_status: blankToNull(detailForm.forSaleStatus),
    media_type_id: blankToNull(detailForm.mediaTypeId),
    art_type_id: blankToNull(detailForm.artTypeId),
    publication_status_id: blankToNull(detailForm.publicationStatusId),
    active: detailForm.active,
    illustration_exchange: detailForm.illustrationExchange,
    ix_for_sale: detailForm.ixForSale,
    artist_credits: detailForm.artistCredits
      .filter((credit) => credit.firstName.trim() || credit.lastName.trim() || credit.roleId.trim())
      .map((credit) => ({
        first_name: blankToNull(credit.firstName),
        last_name: blankToNull(credit.lastName),
        role_id: blankToNull(credit.roleId),
      })),
    media: labelForId(MEDIA_TYPE_OPTIONS, detailForm.mediaTypeId),
    format: labelForId(ART_TYPE_OPTIONS, detailForm.artTypeId),
    caf_url: blankToNull(detailForm.cafUrl),
    snikt_url: blankToNull(detailForm.sniktUrl),
    raremarq_url: blankToNull(detailForm.raremarqUrl),
    generic_url: blankToNull(detailForm.genericUrl),
    snikt_metadata: {
      art_type: blankToNull(detailForm.sniktMetadata.artType),
      comic_publisher: blankToNull(detailForm.sniktMetadata.comicPublisher),
      series_title: blankToNull(detailForm.sniktMetadata.seriesTitle),
      issue_number: blankToNull(detailForm.sniktMetadata.issueNumber),
      series_page_number: blankToNull(detailForm.sniktMetadata.seriesPageNumber),
      year: blankToNull(detailForm.sniktMetadata.year),
      character: blankToNull(detailForm.sniktMetadata.character),
      subcategory: blankToNull(detailForm.sniktMetadata.subcategory),
      animation_studio: blankToNull(detailForm.sniktMetadata.animationStudio),
      episode_number: blankToNull(detailForm.sniktMetadata.episodeNumber),
      episode_title: blankToNull(detailForm.sniktMetadata.episodeTitle),
      published_date: blankToNull(detailForm.sniktMetadata.publishedDate),
      strip_title: blankToNull(detailForm.sniktMetadata.stripTitle),
      is_sunday_strip: detailForm.sniktMetadata.isSundayStrip,
      other: blankToNull(detailForm.sniktMetadata.other),
      tags: blankToNull(detailForm.sniktMetadata.tags),
      is_nsfw: detailForm.sniktMetadata.isNsfw,
      is_for_sale: detailForm.sniktMetadata.isForSale,
      price: blankToNull(detailForm.sniktMetadata.price),
      is_open_to_offers: detailForm.sniktMetadata.isOpenToOffers,
    },
    purchase_price: blankToNull(detailForm.purchasePrice),
    estimated_value: blankToNull(detailForm.estimatedValue),
    purchase_date: blankToNull(detailForm.purchaseDate),
    provenance: blankToNull(detailForm.provenance),
    personal_notes: blankToNull(detailForm.personalNotes),
  };
}

export function metadataAutosaveKey(artworkId: number, detailForm: DetailForm) {
  return JSON.stringify(metadataRequestForForm(artworkId, detailForm));
}
