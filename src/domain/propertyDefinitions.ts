export type PropertySource = "caf" | "snikt" | "raremarq";
export type PropertySourceFilters = Record<PropertySource, boolean>;

export type ProviderFieldCapability = {
  publicImport: boolean;
  csvImport: boolean;
  uploadPrefill: boolean;
};

export type PropertyDefinition = {
  label: string;
  sources: PropertySource[];
  help: string;
  capabilities?: Partial<Record<PropertySource, ProviderFieldCapability>>;
};

const CAF_FULL_CAPABILITY: ProviderFieldCapability = {
  publicImport: true,
  csvImport: true,
  uploadPrefill: false,
};

const CAF_LOCAL_ONLY_CAPABILITY: ProviderFieldCapability = {
  publicImport: false,
  csvImport: false,
  uploadPrefill: false,
};

const CAF_PRIVATE_CAPABILITY: ProviderFieldCapability = {
  publicImport: false,
  csvImport: true,
  uploadPrefill: false,
};

export const PROPERTY_DEFINITIONS: PropertyDefinition[] = [
  {
    label: "CAF URL",
    sources: ["caf"],
    help: "ComicArtFans artwork page URL used to associate this Artwork with CAF. Compatible with: CAF.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "SNIKT URL",
    sources: ["snikt"],
    help: "SNIKT.com image page URL used to associate this Artwork with SNIKT.com. Compatible with: SNIKT.com.",
  },
  {
    label: "Raremarq URL",
    sources: ["raremarq"],
    help: "Raremarq piece page URL used to associate this Artwork with Raremarq. Compatible with: Raremarq.",
  },
  {
    label: "Generic URL",
    sources: [],
    help: "Any additional URL you want to track for this Artwork. The Raremarq bulk upload export wizard can use this URL when populated. Compatible with: OAC local only.",
  },
  {
    label: "Artwork ID",
    sources: [],
    help: "The displayed OAC Artwork identifier, controlled by the current ID preference. Compatible with: OAC local only.",
  },
  {
    label: "Gallery",
    sources: ["caf", "snikt", "raremarq"],
    help: "The OAC Gallery that contains this Artwork. CAF calls this a Gallery Room; SNIKT.com imports map source groupings into OAC Galleries. Compatible with: CAF, SNIKT.com, Raremarq.",
  },
  {
    label: "Title",
    sources: ["caf", "snikt", "raremarq"],
    help: "The public title for the Artwork. Compatible with: CAF, SNIKT.com, Raremarq.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Description",
    sources: ["caf", "snikt", "raremarq"],
    help: "The public artwork description or notes. Compatible with: CAF, SNIKT.com, Raremarq.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "For sale status",
    sources: ["caf"],
    help: "Sale-status text for publishing/export workflows. Compatible with: CAF.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Media type",
    sources: ["caf"],
    help: "CAF controlled media category for the Artwork. Compatible with: CAF.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Artwork type",
    sources: ["caf", "snikt"],
    help: "CAF controlled artwork type/category for the Artwork. Also used as the fallback driver for SNIKT.com extension fields when SNIKT Art type is set to Use OAC artwork type. Compatible with: CAF, SNIKT.com.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Publication status",
    sources: ["caf"],
    help: "CAF publication status for the Artwork, such as published or unpublished art. Compatible with: CAF.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Artist credits",
    sources: ["caf", "snikt", "raremarq"],
    help: "Artists credited on this Artwork. Compatible with: CAF, SNIKT.com, Raremarq.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Artist first name",
    sources: ["caf", "snikt", "raremarq"],
    help: "Artist credit first name. Compatible with: CAF, SNIKT.com, Raremarq.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Artist last name",
    sources: ["caf", "snikt", "raremarq"],
    help: "Artist credit last name. Compatible with: CAF, SNIKT.com, Raremarq.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Artist role",
    sources: ["caf", "snikt"],
    help: "Artist role for this credit. CAF uses controlled role IDs; SNIKT.com uses selected roles such as penciller, inker, and letterer for upload prefill. Compatible with: CAF, SNIKT.com.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Flags",
    sources: ["caf", "snikt"],
    help: "Publishing and exchange flags for this Artwork. CAF supports visibility and Illustration Exchange fields; SNIKT.com supports public visibility in upload prefill. Compatible with: CAF, SNIKT.com.",
  },
  {
    label: "Active",
    sources: ["caf", "snikt"],
    help: "Public visibility flag for whether the Artwork is displayed publicly. Compatible with: CAF, SNIKT.com.",
    capabilities: { caf: CAF_FULL_CAPABILITY },
  },
  {
    label: "Illustration Exchange",
    sources: ["caf"],
    help: "CAF Illustration Exchange flag. Public CAF imports and CAF CSV imports do not expose this field, so OAC preserves it as manually entered CAF-compatible metadata. Compatible with: CAF.",
    capabilities: { caf: CAF_LOCAL_ONLY_CAPABILITY },
  },
  {
    label: "IX for sale",
    sources: ["caf"],
    help: "CAF Illustration Exchange for-sale flag. Public CAF imports and CAF CSV imports do not expose this field, so OAC preserves it as manually entered CAF-compatible metadata. Compatible with: CAF.",
    capabilities: { caf: CAF_LOCAL_ONLY_CAPABILITY },
  },
  {
    label: "SNIKT extension fields",
    sources: ["snikt"],
    help: "SNIKT.com extension fields. OAC sends all known fields in the SNIKT upload-prefill URL; SNIKT may currently ignore some future-compatible fields. Compatible with: SNIKT.com.",
  },
  {
    label: "Art type",
    sources: ["snikt"],
    help: "SNIKT.com art type that controls which upload fields are relevant on SNIKT. Compatible with: SNIKT.com.",
  },
  {
    label: "Publisher",
    sources: ["snikt"],
    help: "Publisher field for SNIKT.com comic cover and interior uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Series title",
    sources: ["snikt"],
    help: "Series, card set, animation, or strip title for SNIKT.com upload prefill. Compatible with: SNIKT.com.",
  },
  {
    label: "Issue number",
    sources: ["snikt"],
    help: "Comic issue number for SNIKT.com cover and interior uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Page number",
    sources: ["snikt"],
    help: "Comic interior page number for SNIKT.com upload prefill. Compatible with: SNIKT.com.",
  },
  {
    label: "Year",
    sources: ["snikt"],
    help: "Year value for SNIKT.com upload prefill. Compatible with: SNIKT.com.",
  },
  {
    label: "Character",
    sources: ["snikt"],
    help: "Character field used by SNIKT.com commissions, trading card art, animation cels, and other illustrations. Compatible with: SNIKT.com.",
  },
  {
    label: "Animation subcategory",
    sources: ["snikt"],
    help: "Animation cel/drawing subcategory for SNIKT.com animation uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Animation studio",
    sources: ["snikt"],
    help: "Animation studio field for SNIKT.com animation uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Episode number",
    sources: ["snikt"],
    help: "Episode number field for SNIKT.com animation uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Episode title",
    sources: ["snikt"],
    help: "Episode title field for SNIKT.com animation uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Published date",
    sources: ["snikt"],
    help: "Published date field for SNIKT.com comic strip uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Strip title",
    sources: ["snikt"],
    help: "Strip title field for SNIKT.com comic strip uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Sunday strip",
    sources: ["snikt"],
    help: "Sunday-strip flag for SNIKT.com comic strip uploads. Compatible with: SNIKT.com.",
  },
  {
    label: "Other",
    sources: ["snikt"],
    help: "Additional Information field for SNIKT.com upload prefill. Compatible with: SNIKT.com.",
  },
  {
    label: "Tags",
    sources: ["snikt"],
    help: "Tag list for SNIKT.com upload prefill. Compatible with: SNIKT.com.",
  },
  {
    label: "NSFW",
    sources: ["snikt"],
    help: "SNIKT.com not-safe-for-work visibility flag. Compatible with: SNIKT.com.",
  },
  {
    label: "For sale",
    sources: ["snikt"],
    help: "SNIKT.com for-sale flag for upload prefill. Compatible with: SNIKT.com.",
  },
  {
    label: "Sale price",
    sources: ["snikt"],
    help: "SNIKT.com sale price for upload prefill. Compatible with: SNIKT.com.",
  },
  {
    label: "Open to offers",
    sources: ["snikt"],
    help: "SNIKT.com open-to-offers flag for upload prefill. Compatible with: SNIKT.com.",
  },
  {
    label: "Private Data",
    sources: [],
    help: "Private collection-management data. CAF treats these fields as private data; OAC keeps them local unless an explicit export includes them. Compatible with: CAF private metadata, OAC local.",
  },
  {
    label: "Purchase price",
    sources: ["caf"],
    help: "Private purchase price for collection management. Compatible with: CAF private metadata, OAC local.",
    capabilities: { caf: CAF_PRIVATE_CAPABILITY },
  },
  {
    label: "Estimated value",
    sources: ["caf", "snikt"],
    help: "Private estimated value for collection management. Compatible with: CAF private metadata, SNIKT.com upload prefill, OAC local.",
    capabilities: { caf: CAF_PRIVATE_CAPABILITY },
  },
  {
    label: "Purchase date",
    sources: ["caf"],
    help: "Private acquisition date for collection management. Compatible with: CAF private metadata, OAC local.",
    capabilities: { caf: CAF_PRIVATE_CAPABILITY },
  },
  {
    label: "Provenance",
    sources: ["caf"],
    help: "Private provenance and acquisition history. Compatible with: CAF private metadata, OAC local.",
    capabilities: { caf: CAF_PRIVATE_CAPABILITY },
  },
  {
    label: "Personal notes",
    sources: ["caf"],
    help: "Private collector notes that are not intended for public publishing. Compatible with: CAF private metadata, OAC local.",
    capabilities: { caf: CAF_PRIVATE_CAPABILITY },
  },
  {
    label: "Collection name",
    sources: ["caf", "snikt", "raremarq"],
    help: "The OAC Collection name. CAF calls this a Collection; SNIKT.com and Raremarq use user-level collection pages. Compatible with: CAF, SNIKT.com, Raremarq.",
  },
  {
    label: "Manifest path",
    sources: [],
    help: "Path to the local human-readable OAC manifest file for this item. Compatible with: OAC local only.",
  },
  {
    label: "CAF Collection ID",
    sources: ["caf"],
    help: "CAF GCat identifier for this OAC Collection. Compatible with: CAF.",
  },
  {
    label: "SNIKT Collection ID",
    sources: ["snikt"],
    help: "SNIKT.com user identifier for this OAC Collection. Compatible with: SNIKT.com.",
  },
  {
    label: "Raremarq Collection ID",
    sources: ["raremarq"],
    help: "Raremarq user slug for this OAC Collection. Compatible with: Raremarq.",
  },
  {
    label: "Galleries",
    sources: [],
    help: "Number of OAC Galleries in this Collection. Compatible with: OAC local only.",
  },
  {
    label: "Artworks",
    sources: [],
    help: "Number of OAC Artworks in this item. Compatible with: OAC local only.",
  },
  {
    label: "Gallery name",
    sources: ["caf", "snikt", "raremarq"],
    help: "The OAC Gallery name. CAF calls this a Gallery Room; SNIKT.com and Raremarq source galleries map into OAC Galleries. Compatible with: CAF, SNIKT.com, Raremarq.",
  },
  {
    label: "Collection",
    sources: [],
    help: "The OAC Collection that contains this Gallery. Compatible with: OAC local only.",
  },
  {
    label: "CAF Gallery Room ID",
    sources: ["caf"],
    help: "CAF GSub identifier for this OAC Gallery. Compatible with: CAF.",
  },
  {
    label: "SNIKT Gallery ID",
    sources: ["snikt"],
    help: "SNIKT.com source gallery identifier for this OAC Gallery. Compatible with: SNIKT.com.",
  },
  {
    label: "Inherit SNIKT Collection ID",
    sources: ["snikt"],
    help: "Controls whether this OAC Gallery uses the open Collection's SNIKT.com ID. Turn it off when this Gallery should not be tracked on SNIKT.com.",
  },
  {
    label: "Raremarq Gallery ID",
    sources: ["raremarq"],
    help: "Raremarq source gallery identifier for this OAC Gallery. Compatible with: Raremarq.",
  },
];

const PROPERTY_DEFINITION_BY_LABEL = new Map(
  PROPERTY_DEFINITIONS.map((definition) => [definition.label, definition]),
);

export function propertyDefinitionForLabel(label: string): PropertyDefinition | null {
  return PROPERTY_DEFINITION_BY_LABEL.get(normalizePropertyLabel(label)) ?? null;
}

export function propertyHelpForLabel(label: string): string {
  return (
    propertyDefinitionForLabel(label)?.help ??
    `${normalizePropertyLabel(label)} metadata field. Compatible with: OAC local only.`
  );
}

export function propertyLabelVisible(label: string, filters: PropertySourceFilters): boolean {
  const sources = propertyDefinitionForLabel(label)?.sources ?? [];
  if (sources.length === 0) return true;
  return sources.some((source) => filters[source]);
}

function normalizePropertyLabel(label: string): string {
  return label.replace(/\s+\d+$/, "");
}
