import next from "eslint-config-next";

const config = [
  ...next,
  { ignores: ["out/**", ".next/**", "node_modules/**", "playwright-report/**", "test-results/**"] },
  {
    rules: {
      // Website tenant tidak boleh mengeksekusi HTML/JS bebas (PRD.md §4.3, ADR-0007).
      "react/no-danger": "error",
      "no-restricted-syntax": [
        "error",
        { selector: "JSXAttribute[name.name='dangerouslySetInnerHTML']", message: "Dilarang (ADR-0007)." },
      ],
    },
  },
];
export default config;
