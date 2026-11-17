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
    backgroundImage: undefined,
    backgroundSize: "cover",
    backgroundPosition: "center",
    backgroundOpacity: 0,
  };

  const defaultProps: ThemePanelProps = {
    config: defaultConfig,
    onChange: vi.fn(),
    onUploadBackground: vi.fn().mockResolvedValue("asset:bg-123"),
    assetUrl: (ref: string) => (ref ? `https://storage.test/${ref}` : undefined),
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

  it("renders background controls, handles upload, and triggers onChange", async () => {
    const onChange = vi.fn();
    const onUploadBackground = vi.fn().mockResolvedValue("asset:bg-uploaded");
    const props: ThemePanelProps = {
      ...defaultProps,
      config: {
        ...defaultConfig,
        backgroundImage: "asset:existing-bg",
        backgroundSize: "cover",
        backgroundPosition: "center",
        backgroundOpacity: 20,
      },
      onChange,
      onUploadBackground,
    };

    const html = renderToStaticMarkup(<ThemePanel {...props} />);
    expect(html).toContain("Gambar latar");
    expect(html).toContain("Hapus latar");
    expect(html).toContain("https://storage.test/asset:existing-bg");
    expect(html).toContain("Penuh");
    expect(html).toContain("Proporsional");
    expect(html).toContain("Motif berulang");
    expect(html).toContain("Tengah");
    expect(html).toContain("Atas");
    expect(html).toContain("Bawah");
    expect(html).toContain("Kiri");
    expect(html).toContain("Kanan");
    expect(html).toContain(
      "Transparansi tembus pandang pada blok website"
    );

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

    // 1. Remove background button
    const removeBtn = findElement(
      tree,
      (el) =>
        el.type === "button" &&
        (el.props?.children === "Hapus latar" ||
          (Array.isArray(el.props?.children) &&
            el.props.children.includes("Hapus latar")))
    );
    expect(removeBtn).toBeTruthy();
    removeBtn.props.onClick();
    expect(onChange).toHaveBeenCalledWith({ backgroundImage: undefined });

    // 2. Upload file input
    const fileInput = findElement(
      tree,
      (el) => el.type === "input" && el.props?.type === "file"
    );
    expect(fileInput).toBeTruthy();
    const dummyFile = new File(["dummy"], "bg.png", { type: "image/png" });
    await fileInput.props.onChange({ target: { files: [dummyFile] } });
    expect(onUploadBackground).toHaveBeenCalledWith(dummyFile);
    expect(onChange).toHaveBeenCalledWith({ backgroundImage: "asset:bg-uploaded" });

    // 3. Opacity slider
    const opacitySlider = findElement(
      tree,
      (el) => el.type === "input" && el.props?.type === "range"
    );
    expect(opacitySlider).toBeTruthy();
    expect(opacitySlider.props.min).toBe(0);
    expect(opacitySlider.props.max).toBe(90);
    opacitySlider.props.onChange({ target: { value: "45" } });
    expect(onChange).toHaveBeenCalledWith({ backgroundOpacity: 45 });

    // 4. Background size select / buttons
    const sizeSelect = findElement(
      tree,
      (el) =>
        el.type === "select" &&
        el.props?.["aria-label"] === "Ukuran gambar latar"
    );
    expect(sizeSelect).toBeTruthy();
    sizeSelect.props.onChange({ target: { value: "repeat" } });
    expect(onChange).toHaveBeenCalledWith({ backgroundSize: "repeat" });

    // 5. Background position select / buttons
    const posSelect = findElement(
      tree,
      (el) =>
        el.type === "select" &&
        el.props?.["aria-label"] === "Posisi gambar latar"
    );
    expect(posSelect).toBeTruthy();
    posSelect.props.onChange({ target: { value: "top" } });
    expect(onChange).toHaveBeenCalledWith({ backgroundPosition: "top" });
  });
});
