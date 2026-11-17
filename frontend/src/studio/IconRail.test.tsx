import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { IconRail, type StudioTab } from "./IconRail";

describe("IconRail component (Studio Enhancement Task 4)", () => {
  it("renders 4 navigation buttons with correct labels and icons", () => {
    const html = renderToStaticMarkup(
      <IconRail activeTab={null} onSelectTab={() => {}} />
    );

    expect(html).toContain("Halaman");
    expect(html).toContain("Blok");
    expect(html).toContain("Desain");
    expect(html).toContain("Coretan");
    expect(html).toContain('aria-label="Bilah alat Studio"');
  });

  it("applies active class and aria-pressed when a tab is selected", () => {
    const tabs: StudioTab[] = ["pages", "elements", "theme", "draw"];

    for (const tab of tabs) {
      const html = renderToStaticMarkup(
        <IconRail activeTab={tab} onSelectTab={() => {}} />
      );
      expect(html).toContain('class="st-rail-btn active"');
      expect(html).toContain('aria-pressed="true"');
    }
  });

  it("renders with no active buttons when activeTab is null", () => {
    const html = renderToStaticMarkup(
      <IconRail activeTab={null} onSelectTab={() => {}} />
    );

    expect(html).not.toContain("st-rail-btn active");
    expect(html).not.toContain('aria-pressed="true"');
  });
});
