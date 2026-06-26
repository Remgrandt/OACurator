export const SNIKT_EXTENSION_FIELD_LABELS = [
  "Art type",
  "Publisher",
  "Series title",
  "Issue number",
  "Page number",
  "Year",
  "Character",
  "Animation subcategory",
  "Animation studio",
  "Episode number",
  "Episode title",
  "Published date",
  "Strip title",
  "Sunday strip",
  "Other",
  "Tags",
  "NSFW",
  "For sale",
  "Sale price",
  "Open to offers",
] as const;

export type SniktExtensionFieldLabel = (typeof SNIKT_EXTENSION_FIELD_LABELS)[number];

export type SniktFieldVisibilityContext = {
  isForSale?: boolean;
};

const SNIKT_COMMON_FIELD_LABELS = new Set<SniktExtensionFieldLabel>([
  "Art type",
  "Tags",
  "NSFW",
  "For sale",
  "Open to offers",
]);

const SNIKT_ART_TYPE_FIELD_LABELS: Readonly<Record<string, readonly SniktExtensionFieldLabel[]>> = {
  Cover: ["Publisher", "Series title", "Issue number", "Other", "Year"],
  Interior: ["Publisher", "Series title", "Issue number", "Page number", "Other", "Year"],
  Commission: ["Other", "Character", "Year"],
  "Trading Card Art": ["Series title", "Character", "Year", "Other"],
  "Animation Cel": [
    "Animation subcategory",
    "Series title",
    "Episode number",
    "Episode title",
    "Animation studio",
    "Year",
    "Character",
  ],
  "Comic Strip": ["Series title", "Strip title", "Published date", "Sunday strip", "Other", "Year"],
  "Other Illustration": ["Character", "Year", "Other"],
};

export function sniktArtTypeForOacArtType(artTypeId: string): string {
  switch (artTypeId) {
    case "1":
      return "Cover";
    case "3":
    case "4":
    case "12":
    case "13":
    case "17":
      return "Interior";
    case "2":
      return "Commission";
    case "10":
      return "Comic Strip";
    case "18":
      return "Animation Cel";
    case "19":
      return "Trading Card Art";
    default:
      return "Other Illustration";
  }
}

export function effectiveSniktArtType(sniktArtType: string, oacArtTypeId: string): string {
  const explicitArtType = sniktArtType.trim();
  return explicitArtType || sniktArtTypeForOacArtType(oacArtTypeId);
}

export function sniktExtensionFieldVisible(
  label: SniktExtensionFieldLabel,
  artType: string,
  context: SniktFieldVisibilityContext = {},
): boolean {
  if (SNIKT_COMMON_FIELD_LABELS.has(label)) {
    return true;
  }
  if (label === "Sale price") {
    return context.isForSale === true;
  }
  const artTypeLabels = SNIKT_ART_TYPE_FIELD_LABELS[artType];
  return artTypeLabels ? artTypeLabels.includes(label) : true;
}
