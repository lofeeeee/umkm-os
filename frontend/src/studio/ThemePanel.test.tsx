import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import {
  ThemePanel,
  type ThemeConfig,
  type ThemePanelProps,
} from "./ThemePanel";

describe("ThemePanel (Studio Enhancement Task 6)", () => {
  const defaultConfig: ThemeConfig = {
    primary: "#4F46E5",
    font: "inter",
    surface: "light",
    textScale: 1,
    navbarStyle: "top",
    navbarSticky: true,
    hideBadge: false,
    backgroundImage: undefined,
    backgroundSize: "cover",
    backgroundPosition: "center",
    backgroundOpacity: 0,
  };

  const defaultProps: ThemePanelProps = {
    config: defaultConfig,
    onChange: vi.fn(),
    onAddBackground: vi.fn(),
  };

  it("renders navbar navigation section and triggers onChange({ navbarStyle: 'side' })", () => {
    const onChange = vi.fn();
    const props = { ...defaultProps, onChange };

    // Verify static markup contains required navbar labels
    const html = renderToStaticMarkup(<ThemePanel {...props} />);
    expect(html).toContain("Bilah atas");
    expect(html).toContain("Bilah samping");
    expect(html).toContain("Navigasi menempel saat digulir");

    // Call component function directly to test interaction handlers
    const tree = ThemePanel(props);
    expect(tree).toBeDefined();

    // Helper to find React element by text or predicate
    function findElement(node: any, predicate: (el: any) => boolean): any {
      if (!node) return null;
      if (predicate(node)) return node;
      if (node.props?.children) {
        const children = Array.isArray(node.props.children)
          ? node.props.children
          : [node.props.children];
        for (const child of children) {
          const found = findElement(child, predicate);
          if (found) return found;
        }
      }
      return null;
    }

    // Find "Bilah samping" button
    const sideBarBtn = findElement(
      tree,
      (el) =>
        el.type === "button" &&
        (el.props?.children === "Bilah samping" ||
          (Array.isArray(el.props?.children) &&
            el.props.children.includes("Bilah samping")))
    );
    expect(sideBarBtn).toBeTruthy();
    sideBarBtn.props.onClick();
    expect(onChange).toHaveBeenCalledWith({ navbarStyle: "side" });
  });

  it("triggers onChange for sticky navbar toggle", () => {
    const onChange = vi.fn();
    const props = {
      ...defaultProps,
      config: { ...defaultConfig, navbarSticky: true },
      onChange,
    };

    const tree = ThemePanel(props);
    function findElement(node: any, predicate: (el: any) => boolean): any {
      if (!node) return null;
      if (predicate(node)) return node;
      if (node.props?.children) {
        const children = Array.isArray(node.props.children)
          ? node.props.children
          : [node.props.children];
        for (const child of children) {
          const found = findElement(child, predicate);
          if (found) return found;
        }
      }
      return null;
    }

    const stickyInput = findElement(
      tree,
      (el) =>
        el.type === "input" &&
        el.props?.type === "checkbox" &&
        el.props?.["aria-label"] === "Navigasi menempel saat digulir"
    );
    expect(stickyInput).toBeTruthy();
    stickyInput.props.onChange({ target: { checked: false } });
    expect(onChange).toHaveBeenCalledWith({ navbarSticky: false });
  });

  it("renders color palettes, color picker, and surface options", () => {
    const onChange = vi.fn();
    const props = { ...defaultProps, onChange };

    const html = renderToStaticMarkup(<ThemePanel {...props} />);
    expect(html).toContain("Senja");
    expect(html).toContain("Indigo");
    expect(html).toContain("Daun");
    expect(html).toContain("Laut");
    expect(html).toContain("Mawar");
    expect(html).toContain("Kopi");
    expect(html).toContain("Malam");
    expect(html).toContain("Emas");
    expect(html).toContain('type="color"');
    expect(html).toContain("Putih");
    expect(html).toContain("Krem");
    expect(html).toContain("Gelap");

    const tree = ThemePanel(props);
    function findElement(node: any, predicate: (el: any) => boolean): any {
      if (!node) return null;
      if (predicate(node)) return node;
      if (node.props?.children) {
        const children = Array.isArray(node.props.children)
          ? node.props.children
          : [node.props.children];
        for (const child of children) {
          const found = findElement(child, predicate);
          if (found) return found;
        }
      }
      return null;
    }

    // Test picking "Senja" palette (primary: #C2410C, surface: cream)
    const senjaSwatch = findElement(
      tree,
      (el) => el.type === "button" && el.props?.title === "Senja"
    );
    expect(senjaSwatch).toBeTruthy();
    senjaSwatch.props.onClick();
    expect(onChange).toHaveBeenCalledWith({
      primary: "#C2410C",
      surface: "cream",
    });
  });

  it("renders typography options and triggers textScale change", () => {
    const onChange = vi.fn();
    const props = { ...defaultProps, onChange };

    const html = renderToStaticMarkup(<ThemePanel {...props} />);
    expect(html).toContain("Font");
    expect(html).toContain("Ukuran teks");
    expect(html).toContain("Kecil");
    expect(html).toContain("Sedang");
    expect(html).toContain("Besar");

    const tree = ThemePanel(props);
    function findElement(node: any, predicate: (el: any) => boolean): any {
      if (!node) return null;
      if (predicate(node)) return node;
      if (node.props?.children) {
        const children = Array.isArray(node.props.children)
          ? node.props.children
          : [node.props.children];
        for (const child of children) {
          const found = findElement(child, predicate);
          if (found) return found;
        }
      }
      return null;
    }

    const besarBtn = findElement(
      tree,
      (el) =>
        el.type === "button" &&
        findElement(el, (c) => c.props?.children === "Besar")
    );
    expect(besarBtn).toBeTruthy();
    besarBtn.props.onClick();
    expect(onChange).toHaveBeenCalledWith({ textScale: 1.1 });
  });

  it("latar diarahkan ke plugin Latar; latar tema lama bisa dipindahkan", () => {
    const fresh = renderToStaticMarkup(<ThemePanel {...defaultProps} />);
    expect(fresh).toContain("Tambah plugin Latar");
    expect(fresh).not.toContain("Pindahkan ke plugin Latar");
    const legacy = renderToStaticMarkup(<ThemePanel {...defaultProps} config={{ ...defaultConfig, backgroundImage: "asset:lama" }} />);
    expect(legacy).toContain("Pindahkan ke plugin Latar");
    expect(legacy).not.toContain("Kepekatan gambar latar");
  });
});
