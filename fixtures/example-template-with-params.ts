import {
  Notebook,
  TextCell,
  CodeCell,
  PrometheusCell,
} from "https://raw.githubusercontent.com/keturiosakys/template-api/main/mod.ts";

export default function Template({
  title,
  message,
}: {
  title: string;
  message: string;
}) {
  return Notebook({
    title,
    frontMatter: {},
    cells: [
      TextCell(message),
      CodeCell("python", "print('Hello, world!')"),
      PrometheusCell("up"),
    ],
  });
}
