export const DEMO_SELECT_VISIBLE_OPTION_LIMIT = 8;

export type DemoSelectVisibleWindow = {
  options: string[];
  selectedIndex: number;
};

export function demoSelectVisibleWindow(
  options: string[],
  selected: string,
  limit = DEMO_SELECT_VISIBLE_OPTION_LIMIT,
): DemoSelectVisibleWindow {
  const cleanedOptions = options.filter((option) => option.trim().length > 0);
  const selectedOption = selected.trim();
  const normalizedOptions =
    selectedOption && !cleanedOptions.includes(selectedOption)
      ? [...cleanedOptions, selectedOption]
      : cleanedOptions;
  if (normalizedOptions.length === 0) {
    return { options: selectedOption ? [selectedOption] : [], selectedIndex: 0 };
  }

  const fullSelectedIndex = Math.max(
    0,
    normalizedOptions.findIndex((option) => option === selectedOption),
  );
  const visibleLimit = Math.max(1, Math.floor(limit));
  if (normalizedOptions.length <= visibleLimit) {
    return { options: normalizedOptions, selectedIndex: fullSelectedIndex };
  }

  const preferredBeforeSelected = Math.floor((visibleLimit - 1) / 2);
  const maxStart = normalizedOptions.length - visibleLimit;
  const start = Math.min(Math.max(0, fullSelectedIndex - preferredBeforeSelected), maxStart);

  return {
    options: normalizedOptions.slice(start, start + visibleLimit),
    selectedIndex: fullSelectedIndex - start,
  };
}
